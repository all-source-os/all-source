use crate::{
    domain::entities::Event,
    error::Result,
    infrastructure::{
        hash_state::{FastDashMap, fast_dashmap},
        memory::ledger::{MapCharge, Owner, Tally},
        observability::{heap::json_heap_bytes, metrics::MetricsRegistry},
    },
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    mem::size_of,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

/// A projection aggregates events into a queryable view
pub trait Projection: Send + Sync {
    /// Get the name of this projection
    fn name(&self) -> &str;

    /// Process an event and update the projection state
    fn process(&self, event: &Event) -> Result<()>;

    /// Get the current state of the projection for an entity
    fn get_state(&self, entity_id: &str) -> Option<Value>;

    /// Clear all projection state
    fn clear(&self);

    /// Snapshot the entire projection state for checkpointing.
    /// Returns `None` if the projection does not support checkpointing.
    fn snapshot(&self) -> Option<Value> {
        None
    }

    /// Restore projection state from a previously saved snapshot.
    fn restore(&self, _snapshot: &Value) -> Result<()> {
        Ok(())
    }
}

/// A checkpoint of a projection's state, persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectionCheckpoint {
    pub projection_name: String,
    pub state: Value,
    pub last_event_timestamp: DateTime<Utc>,
    pub event_count: u64,
}

/// Configuration for projection checkpointing behaviour.
#[derive(Debug, Clone)]
pub struct CheckpointConfig {
    pub enabled: bool,
    /// Checkpoint after every N events processed by a projection.
    pub interval_events: u64,
    /// Checkpoint after M seconds since the last checkpoint.
    pub interval_seconds: u64,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_events: 10_000,
            interval_seconds: 300,
        }
    }
}

/// Per-projection tracking state for checkpointing.
struct ProjectionState {
    projection: Arc<dyn Projection>,
    events_since_checkpoint: AtomicU64,
    total_event_count: AtomicU64,
    /// The timestamp of the checkpoint that was restored (if any).
    /// Events at or before this timestamp are skipped during replay.
    restored_up_to: parking_lot::Mutex<Option<DateTime<Utc>>>,
}

/// Entity snapshot projection - maintains current state of each entity
pub struct EntitySnapshotProjection {
    name: String,
    /// entity_id -> (latest state, last event timestamp)
    states: Arc<FastDashMap<String, (Value, DateTime<Utc>)>>,
    states_charge: MapCharge,
    states_owned: Tally,
}

impl EntitySnapshotProjection {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            states: Arc::new(fast_dashmap()),
            states_charge: MapCharge::new(
                Owner::ProjectionPermanent,
                size_of::<(String, (Value, DateTime<Utc>))>(),
            ),
            states_owned: Tally::new(Owner::ProjectionPermanent),
        }
    }

    fn charge_state(&self, key: &str, state: &Value) {
        self.states_owned
            .add(key.len() as u64 + json_heap_bytes(state) as u64);
    }

    /// Get all entity states
    pub fn get_all_states(&self) -> Vec<(String, Value)> {
        self.states
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().0.clone()))
            .collect()
    }
}

impl Projection for EntitySnapshotProjection {
    fn name(&self) -> &str {
        &self.name
    }

    fn process(&self, event: &Event) -> Result<()> {
        // Timestamp-aware merge strategy: only merge if the event is at least
        // as new as the last processed event for this entity. This ensures
        // convergence during bidirectional sync where events may arrive out of
        // order. In the normal (non-sync) case, events are always in order so
        // this condition is always true.
        let key = event.entity_id_str();
        if !self.states.contains_key(key) {
            self.states_charge.ensure(
                &self.states,
                key,
                || key.to_string(),
                || (event.payload.clone(), event.timestamp),
                0,
            );
            self.charge_state(key, &event.payload);
        }
        self.states
            .entry(key.to_string())
            .and_modify(|(state, last_ts)| {
                if event.timestamp >= *last_ts {
                    // Merge the event payload into existing state
                    let before = json_heap_bytes(state) as u64;
                    if let (Value::Object(map), Value::Object(payload_map)) =
                        (&mut *state, &event.payload)
                    {
                        for (key, value) in payload_map {
                            map.insert(key.clone(), value.clone());
                        }
                    }
                    self.states_owned
                        .adjust(before, json_heap_bytes(state) as u64);
                    *last_ts = event.timestamp;
                }
                // Out-of-order event: skip merge to ensure convergence
            })
            .or_insert_with(|| (event.payload.clone(), event.timestamp));

        Ok(())
    }

    fn get_state(&self, entity_id: &str) -> Option<Value> {
        self.states.get(entity_id).map(|v| v.0.clone())
    }

    fn clear(&self) {
        self.states.clear();
        self.states_owned.take();
    }

    fn snapshot(&self) -> Option<Value> {
        let entries: Vec<(String, Value, DateTime<Utc>)> = self
            .states
            .iter()
            .map(|entry| {
                let (state, ts) = entry.value().clone();
                (entry.key().clone(), state, ts)
            })
            .collect();
        serde_json::to_value(entries).ok()
    }

    fn restore(&self, snapshot: &Value) -> Result<()> {
        let entries: Vec<(String, Value, DateTime<Utc>)> = serde_json::from_value(snapshot.clone())
            .map_err(|e| crate::error::AllSourceError::StorageError(e.to_string()))?;
        self.states.clear();
        self.states_owned.take();
        for (key, state, ts) in entries {
            self.charge_state(&key, &state);
            self.states.insert(key, (state, ts));
        }
        Ok(())
    }
}

/// Event counter projection - counts events by type
pub struct EventCounterProjection {
    name: String,
    /// event_type -> count
    counts: Arc<FastDashMap<String, u64>>,
    counts_charge: MapCharge,
}

impl EventCounterProjection {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            counts: Arc::new(fast_dashmap()),
            counts_charge: MapCharge::new(
                Owner::ProjectionEventCounters,
                size_of::<(String, u64)>(),
            ),
        }
    }

    /// Get count for a specific event type
    pub fn get_count(&self, event_type: &str) -> u64 {
        self.counts.get(event_type).map_or(0, |v| *v)
    }

    /// Get all event type counts
    pub fn get_all_counts(&self) -> Vec<(String, u64)> {
        self.counts
            .iter()
            .map(|entry| (entry.key().clone(), *entry.value()))
            .collect()
    }
}

impl Projection for EventCounterProjection {
    fn name(&self) -> &str {
        &self.name
    }

    fn process(&self, event: &Event) -> Result<()> {
        let key = event.event_type_str();
        self.counts_charge.ensure(
            &self.counts,
            key,
            || key.to_string(),
            || 0,
            key.len() as u64,
        );
        self.counts
            .entry(key.to_string())
            .and_modify(|count| *count += 1)
            .or_insert(1);

        Ok(())
    }

    fn get_state(&self, event_type: &str) -> Option<Value> {
        self.counts
            .get(event_type)
            .map(|count| serde_json::json!({ "count": *count }))
    }

    fn clear(&self) {
        self.counts.clear();
        self.counts_charge.clear_owned();
    }

    fn snapshot(&self) -> Option<Value> {
        let entries: Vec<(String, u64)> = self
            .counts
            .iter()
            .map(|entry| (entry.key().clone(), *entry.value()))
            .collect();
        serde_json::to_value(entries).ok()
    }

    fn restore(&self, snapshot: &Value) -> Result<()> {
        let entries: Vec<(String, u64)> = serde_json::from_value(snapshot.clone())
            .map_err(|e| crate::error::AllSourceError::StorageError(e.to_string()))?;
        self.counts.clear();
        self.counts_charge.clear_owned();
        for (key, count) in entries {
            self.counts_charge.ensure(
                &self.counts,
                key.as_str(),
                || key.clone(),
                || count,
                key.len() as u64,
            );
            self.counts.insert(key, count);
        }
        Ok(())
    }
}

/// Projection manager handles multiple projections
pub struct ProjectionManager {
    states: Vec<ProjectionState>,
    metrics: Arc<MetricsRegistry>,
    checkpoint_config: CheckpointConfig,
    checkpoint_dir: Option<PathBuf>,
    last_checkpoint_time: parking_lot::Mutex<Instant>,
}

impl ProjectionManager {
    pub fn new() -> Self {
        Self::with_metrics(MetricsRegistry::new())
    }

    pub fn with_metrics(metrics: Arc<MetricsRegistry>) -> Self {
        Self {
            states: Vec::new(),
            metrics,
            checkpoint_config: CheckpointConfig::default(),
            checkpoint_dir: None,
            last_checkpoint_time: parking_lot::Mutex::new(Instant::now()),
        }
    }

    /// Set checkpoint configuration and directory.
    pub fn with_checkpoint_config(mut self, config: CheckpointConfig, dir: PathBuf) -> Self {
        self.checkpoint_config = config;
        self.checkpoint_dir = Some(dir);
        self
    }

    /// Register a new projection. If a checkpoint file exists on disk, restore
    /// from it so that only events newer than `last_event_timestamp` need to be
    /// replayed.
    pub fn register(&mut self, projection: Arc<dyn Projection>) {
        let name = projection.name().to_string();
        tracing::info!("Registering projection: {name}");

        let state = ProjectionState {
            projection,
            events_since_checkpoint: AtomicU64::new(0),
            total_event_count: AtomicU64::new(0),
            restored_up_to: parking_lot::Mutex::new(None),
        };

        // Attempt to restore from checkpoint
        if self.checkpoint_config.enabled
            && let Some(path) = self.checkpoint_path(&name)
            && path.exists()
        {
            self.try_restore_checkpoint(&path, state.projection.as_ref(), &state);
        }

        self.states.push(state);
        self.metrics.projections_total.set(self.states.len() as i64);
    }

    /// Try to read a checkpoint file and restore the projection from it.
    fn try_restore_checkpoint(
        &self,
        path: &std::path::Path,
        projection: &dyn Projection,
        state: &ProjectionState,
    ) {
        let name = projection.name();
        let data = match std::fs::read_to_string(path) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("Failed to read checkpoint file for projection '{name}': {e}");
                return;
            }
        };
        let checkpoint: ProjectionCheckpoint = match serde_json::from_str(&data) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to parse checkpoint for projection '{name}': {e}");
                return;
            }
        };
        match projection.restore(&checkpoint.state) {
            Ok(()) => {
                state
                    .total_event_count
                    .store(checkpoint.event_count, Ordering::Relaxed);
                *state.restored_up_to.lock() = Some(checkpoint.last_event_timestamp);
                let count = checkpoint.event_count;
                let ts = checkpoint.last_event_timestamp;
                tracing::info!(
                    "Restored projection '{name}' from checkpoint (event_count={count}, up_to={ts})",
                );
            }
            Err(e) => {
                tracing::warn!("Failed to restore projection '{name}' from checkpoint: {e}",);
            }
        }
    }

    /// Process an event through all projections.
    /// Events at or before a projection's restored checkpoint timestamp are
    /// skipped (they were already folded into the restored state).
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn process_event(&self, event: &Event) -> Result<()> {
        let timer = self.metrics.projection_duration_seconds.start_timer();

        for state in &self.states {
            let name = state.projection.name();

            // Skip events that are already covered by the restored checkpoint.
            {
                let restored = state.restored_up_to.lock();
                if restored.is_some_and(|up_to| event.timestamp <= up_to) {
                    continue;
                }
            }

            match state.projection.process(event) {
                Ok(()) => {
                    self.metrics
                        .projection_events_processed
                        .with_label_values(&[name])
                        .inc();
                    state
                        .events_since_checkpoint
                        .fetch_add(1, Ordering::Relaxed);
                    state.total_event_count.fetch_add(1, Ordering::Relaxed);
                }
                Err(e) => {
                    self.metrics
                        .projection_errors_total
                        .with_label_values(&[name])
                        .inc();
                    tracing::error!(
                        "Projection '{name}' failed to process event {}: {e}",
                        event.id,
                    );
                    // Continue processing other projections even if one fails
                }
            }
        }

        // Check if we should write a checkpoint (by event count or time).
        if self.checkpoint_config.enabled && self.checkpoint_dir.is_some() {
            self.maybe_checkpoint();
        }

        timer.observe_duration();
        Ok(())
    }

    /// Check whether the event or time interval threshold has been reached
    /// and checkpoint all projections if so.
    fn maybe_checkpoint(&self) {
        let events_threshold = self.checkpoint_config.interval_events;
        let time_threshold = self.checkpoint_config.interval_seconds;

        let any_exceeded = self
            .states
            .iter()
            .any(|s| s.events_since_checkpoint.load(Ordering::Relaxed) >= events_threshold);

        let time_exceeded = {
            let last = self.last_checkpoint_time.lock();
            last.elapsed().as_secs() >= time_threshold
        };

        if (any_exceeded || time_exceeded) && self.checkpoint_all().is_err() {
            tracing::error!("Failed to write projection checkpoints");
        }
    }

    /// Checkpoint all projections that support snapshotting.
    pub fn checkpoint_all(&self) -> Result<()> {
        for state in &self.states {
            self.checkpoint_one(state)?;
        }
        *self.last_checkpoint_time.lock() = Instant::now();
        Ok(())
    }

    /// Checkpoint a single projection.
    fn checkpoint_one(&self, state: &ProjectionState) -> Result<()> {
        let name = state.projection.name();
        let Some(snapshot) = state.projection.snapshot() else {
            return Ok(());
        };

        let Some(path) = self.checkpoint_path(name) else {
            return Ok(());
        };

        let checkpoint = ProjectionCheckpoint {
            projection_name: name.to_string(),
            state: snapshot,
            last_event_timestamp: Utc::now(),
            event_count: state.total_event_count.load(Ordering::Relaxed),
        };

        let json = serde_json::to_string_pretty(&checkpoint)
            .map_err(|e| crate::error::AllSourceError::StorageError(e.to_string()))?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| crate::error::AllSourceError::StorageError(e.to_string()))?;
        }

        std::fs::write(&path, json)
            .map_err(|e| crate::error::AllSourceError::StorageError(e.to_string()))?;

        state.events_since_checkpoint.store(0, Ordering::Relaxed);

        tracing::debug!("Checkpointed projection '{name}'");
        Ok(())
    }

    /// Build the checkpoint file path for a projection.
    fn checkpoint_path(&self, name: &str) -> Option<PathBuf> {
        self.checkpoint_dir
            .as_ref()
            .map(|dir| dir.join(format!("{name}.checkpoint.json")))
    }

    /// Get a projection by name
    pub fn get_projection(&self, name: &str) -> Option<Arc<dyn Projection>> {
        self.states
            .iter()
            .find(|s| s.projection.name() == name)
            .map(|s| Arc::clone(&s.projection))
    }

    /// List all projections
    pub fn list_projections(&self) -> Vec<(String, Arc<dyn Projection>)> {
        self.states
            .iter()
            .map(|s| (s.projection.name().to_string(), Arc::clone(&s.projection)))
            .collect()
    }

    /// Clear all projections
    pub fn clear_all(&self) {
        for state in &self.states {
            state.projection.clear();
        }
    }

    /// Return the restored-up-to timestamp for a given projection (if any).
    /// Useful in tests to verify that a checkpoint was restored.
    pub fn restored_up_to(&self, name: &str) -> Option<DateTime<Utc>> {
        self.states
            .iter()
            .find(|s| s.projection.name() == name)
            .and_then(|s| *s.restored_up_to.lock())
    }
}

impl Default for ProjectionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn create_test_event(entity_id: &str, event_type: &str) -> Event {
        Event::reconstruct_from_strings(
            Uuid::new_v4(),
            event_type.to_string(),
            entity_id.to_string(),
            "default".to_string(),
            serde_json::json!({
                "name": "Test User",
                "email": "test@example.com"
            }),
            chrono::Utc::now(),
            None,
            1,
        )
    }

    fn create_test_event_with_timestamp(
        entity_id: &str,
        event_type: &str,
        timestamp: DateTime<Utc>,
    ) -> Event {
        Event::reconstruct_from_strings(
            Uuid::new_v4(),
            event_type.to_string(),
            entity_id.to_string(),
            "default".to_string(),
            serde_json::json!({
                "name": "Test User",
                "email": "test@example.com"
            }),
            timestamp,
            None,
            1,
        )
    }

    #[test]
    fn test_entity_snapshot_projection() {
        let projection = EntitySnapshotProjection::new("test");
        let event = create_test_event("user-123", "user.created");

        projection.process(&event).unwrap();

        let state = projection.get_state("user-123").unwrap();
        assert_eq!(state["name"], "Test User");
    }

    #[test]
    fn test_event_counter_projection() {
        let projection = EventCounterProjection::new("counter");

        let event1 = create_test_event("user-123", "user.created");
        let event2 = create_test_event("user-456", "user.created");
        let event3 = create_test_event("user-123", "user.updated");

        projection.process(&event1).unwrap();
        projection.process(&event2).unwrap();
        projection.process(&event3).unwrap();

        assert_eq!(projection.get_count("user.created"), 2);
        assert_eq!(projection.get_count("user.updated"), 1);
    }

    #[test]
    fn test_projection_manager() {
        let mut manager = ProjectionManager::new();

        let snapshot = Arc::new(EntitySnapshotProjection::new("snapshot"));
        let counter = Arc::new(EventCounterProjection::new("counter"));

        manager.register(snapshot.clone());
        manager.register(counter.clone());

        let event = create_test_event("user-123", "user.created");
        manager.process_event(&event).unwrap();

        assert!(snapshot.get_state("user-123").is_some());
        assert_eq!(counter.get_count("user.created"), 1);
    }

    #[test]
    fn test_entity_snapshot_snapshot_restore() {
        let projection = EntitySnapshotProjection::new("snap");
        let event = create_test_event("user-1", "user.created");
        projection.process(&event).unwrap();

        let snap = projection.snapshot().expect("snapshot should be Some");

        let projection2 = EntitySnapshotProjection::new("snap");
        projection2.restore(&snap).unwrap();

        let state = projection2.get_state("user-1").unwrap();
        assert_eq!(state["name"], "Test User");
    }

    #[test]
    fn test_event_counter_snapshot_restore() {
        let projection = EventCounterProjection::new("counter");
        for _ in 0..5 {
            let event = create_test_event("user-1", "user.created");
            projection.process(&event).unwrap();
        }

        let snap = projection.snapshot().expect("snapshot should be Some");

        let projection2 = EventCounterProjection::new("counter");
        projection2.restore(&snap).unwrap();

        assert_eq!(projection2.get_count("user.created"), 5);
    }

    #[test]
    fn test_checkpoint_write_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_dir = dir.path().join("projections");

        let config = CheckpointConfig {
            enabled: true,
            interval_events: 100,
            interval_seconds: 3600,
        };

        let mut manager =
            ProjectionManager::new().with_checkpoint_config(config, checkpoint_dir.clone());

        let counter = Arc::new(EventCounterProjection::new("counter"));
        manager.register(counter.clone());

        // Process some events
        for i in 0..50 {
            let event = create_test_event(&format!("user-{i}"), "user.created");
            manager.process_event(&event).unwrap();
        }

        // Manually trigger checkpoint
        manager.checkpoint_all().unwrap();

        // Verify the file was written
        let cp_path = checkpoint_dir.join("counter.checkpoint.json");
        assert!(cp_path.exists());

        // Read and verify the checkpoint content
        let data = std::fs::read_to_string(&cp_path).unwrap();
        let checkpoint: ProjectionCheckpoint = serde_json::from_str(&data).unwrap();
        assert_eq!(checkpoint.projection_name, "counter");
        assert_eq!(checkpoint.event_count, 50);
    }

    #[test]
    fn test_checkpoint_restore_on_register() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_dir = dir.path().join("projections");

        let config = CheckpointConfig {
            enabled: true,
            interval_events: 100_000,
            interval_seconds: 3600,
        };

        // Phase 1: process events and checkpoint
        {
            let mut manager = ProjectionManager::new()
                .with_checkpoint_config(config.clone(), checkpoint_dir.clone());

            let counter = Arc::new(EventCounterProjection::new("counter"));
            manager.register(counter.clone());

            for i in 0..100 {
                let event = create_test_event(&format!("user-{i}"), "user.created");
                manager.process_event(&event).unwrap();
            }

            manager.checkpoint_all().unwrap();

            assert_eq!(counter.get_count("user.created"), 100);
        }

        // Phase 2: create new manager, register same projection, verify restore
        {
            let mut manager =
                ProjectionManager::new().with_checkpoint_config(config, checkpoint_dir);

            let counter = Arc::new(EventCounterProjection::new("counter"));
            manager.register(counter.clone());

            // The projection should have been restored from the checkpoint
            assert_eq!(counter.get_count("user.created"), 100);

            // The manager should have recorded that events up to the checkpoint
            // timestamp can be skipped
            assert!(manager.restored_up_to("counter").is_some());
        }
    }

    #[test]
    fn test_checkpoint_skips_old_events_during_replay() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_dir = dir.path().join("projections");

        let config = CheckpointConfig {
            enabled: true,
            interval_events: 100_000,
            interval_seconds: 3600,
        };

        let checkpoint_time = Utc::now();

        // Phase 1: process 1000 events and checkpoint
        {
            let mut manager = ProjectionManager::new()
                .with_checkpoint_config(config.clone(), checkpoint_dir.clone());

            let counter = Arc::new(EventCounterProjection::new("counter"));
            manager.register(counter.clone());

            for i in 0..1000 {
                let event = create_test_event_with_timestamp(
                    &format!("user-{i}"),
                    "user.created",
                    checkpoint_time,
                );
                manager.process_event(&event).unwrap();
            }

            manager.checkpoint_all().unwrap();
            assert_eq!(counter.get_count("user.created"), 1000);
        }

        // Phase 2: simulate restart — restore from checkpoint and replay
        {
            let mut manager =
                ProjectionManager::new().with_checkpoint_config(config, checkpoint_dir);

            let counter = Arc::new(EventCounterProjection::new("counter"));
            manager.register(counter.clone());

            // Counter should be restored
            assert_eq!(counter.get_count("user.created"), 1000);

            // Replay the old 1000 events (should be skipped because their
            // timestamp <= checkpoint timestamp)
            for i in 0..1000 {
                let event = create_test_event_with_timestamp(
                    &format!("user-{i}"),
                    "user.created",
                    checkpoint_time,
                );
                manager.process_event(&event).unwrap();
            }

            // Count should still be 1000 — old events were skipped
            assert_eq!(counter.get_count("user.created"), 1000);

            // Now process 10 NEW events with a later timestamp
            let later = checkpoint_time + chrono::Duration::seconds(10);
            for i in 0..10 {
                let event = create_test_event_with_timestamp(
                    &format!("new-user-{i}"),
                    "user.created",
                    later,
                );
                manager.process_event(&event).unwrap();
            }

            // Now we should have 1010 total
            assert_eq!(counter.get_count("user.created"), 1010);
        }
    }

    #[test]
    fn test_auto_checkpoint_by_event_count() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_dir = dir.path().join("projections");

        let config = CheckpointConfig {
            enabled: true,
            interval_events: 50, // checkpoint every 50 events
            interval_seconds: 3600,
        };

        let mut manager =
            ProjectionManager::new().with_checkpoint_config(config, checkpoint_dir.clone());

        let counter = Arc::new(EventCounterProjection::new("counter"));
        manager.register(counter.clone());

        // Process 60 events — should trigger auto-checkpoint at 50
        for i in 0..60 {
            let event = create_test_event(&format!("user-{i}"), "user.created");
            manager.process_event(&event).unwrap();
        }

        // Checkpoint file should exist
        let cp_path = checkpoint_dir.join("counter.checkpoint.json");
        assert!(cp_path.exists());
    }

    #[test]
    fn test_default_noop_snapshot_restore() {
        // A projection that does NOT implement snapshot/restore should use the
        // default no-op implementations and not break anything.
        struct MinimalProjection;
        impl Projection for MinimalProjection {
            fn name(&self) -> &'static str {
                "minimal"
            }
            fn process(&self, _event: &Event) -> Result<()> {
                Ok(())
            }
            fn get_state(&self, _entity_id: &str) -> Option<Value> {
                None
            }
            fn clear(&self) {}
        }

        let projection = MinimalProjection;
        assert!(projection.snapshot().is_none());
        assert!(projection.restore(&Value::Null).is_ok());
    }

    #[test]
    fn test_checkpoint_with_no_dir_is_noop() {
        // When no checkpoint_dir is configured, checkpoint operations should
        // succeed silently (no-op).
        let mut manager = ProjectionManager::new();
        let counter = Arc::new(EventCounterProjection::new("counter"));
        manager.register(counter.clone());

        for _ in 0..10 {
            let event = create_test_event("user-1", "user.created");
            manager.process_event(&event).unwrap();
        }

        // Should not error even without a checkpoint directory
        manager.checkpoint_all().unwrap();
    }
}
