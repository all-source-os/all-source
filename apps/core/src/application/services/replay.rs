use crate::{
    application::{dto::QueryEventsRequest, services::projection::Projection},
    domain::entities::Event,
    error::{AllSourceError, Result},
    infrastructure::memory::ledger::{Owner, Tally, option_string_heap},
    store::EventStore,
};
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::{
    mem::size_of,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use uuid::Uuid;

/// Status of a replay operation
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReplayStatus {
    /// Replay is pending and hasn't started yet
    Pending,
    /// Replay is currently running
    Running,
    /// Replay completed successfully
    Completed,
    /// Replay failed with an error
    Failed,
    /// Replay was cancelled by user
    Cancelled,
}

/// Configuration for replay operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayConfig {
    /// Batch size for processing events
    pub batch_size: usize,

    /// Whether to run replay in parallel
    pub parallel: bool,

    /// Number of parallel workers (if parallel is true)
    pub workers: usize,

    /// Whether to emit progress events
    pub emit_progress: bool,

    /// Progress reporting interval (every N events)
    pub progress_interval: usize,
}

impl Default for ReplayConfig {
    fn default() -> Self {
        Self {
            batch_size: 1000,
            parallel: false,
            workers: 4,
            emit_progress: true,
            progress_interval: 1000,
        }
    }
}

/// Request to start a replay operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartReplayRequest {
    /// Optional projection name to rebuild (if None, replays all projections)
    pub projection_name: Option<String>,

    /// Start from this timestamp (if None, starts from beginning)
    pub from_timestamp: Option<DateTime<Utc>>,

    /// End at this timestamp (if None, goes to end)
    pub to_timestamp: Option<DateTime<Utc>>,

    /// Filter by entity_id (optional)
    pub entity_id: Option<String>,

    /// Filter by event_type (optional)
    pub event_type: Option<String>,

    /// Replay configuration
    pub config: Option<ReplayConfig>,
}

/// Response from starting a replay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartReplayResponse {
    pub replay_id: Uuid,
    pub status: ReplayStatus,
    pub started_at: DateTime<Utc>,
    pub total_events: usize,
}

/// Progress information for a replay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayProgress {
    pub replay_id: Uuid,
    pub status: ReplayStatus,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub total_events: usize,
    pub processed_events: usize,
    pub failed_events: usize,
    pub progress_percentage: f64,
    pub events_per_second: f64,
    pub error_message: Option<String>,
}

/// Manages event replay and projection rebuilding
pub struct ReplayManager {
    /// Active replay operations
    replays: Arc<RwLock<Vec<ReplayState>>>,
    /// The history vector's buffer and each state's owned heap.
    replays_charge: Tally,
}

/// Heap of one `Arc` allocation holding a `T`: the value and the two counts.
const fn arc_bytes<T>() -> u64 {
    (size_of::<T>() + 2 * size_of::<usize>()) as u64
}

/// Owned heap of a replay state: its projection name and its shared cells.
fn replay_heap(state: &ReplayState) -> u64 {
    option_string_heap(&state.projection_name)
        + arc_bytes::<RwLock<ReplayStatus>>()
        + arc_bytes::<RwLock<Option<DateTime<Utc>>>>()
        + 2 * arc_bytes::<AtomicU64>()
        + arc_bytes::<RwLock<Option<String>>>()
        + arc_bytes::<AtomicBool>()
}

/// Internal state for a replay operation
struct ReplayState {
    id: Uuid,
    projection_name: Option<String>,
    status: Arc<RwLock<ReplayStatus>>,
    started_at: DateTime<Utc>,
    completed_at: Arc<RwLock<Option<DateTime<Utc>>>>,
    total_events: usize,
    processed_events: Arc<AtomicU64>,
    failed_events: Arc<AtomicU64>,
    error_message: Arc<RwLock<Option<String>>>,
    cancelled: Arc<AtomicBool>,
}

impl ReplayManager {
    pub fn new() -> Self {
        Self {
            replays: Arc::new(RwLock::new(Vec::new())),
            replays_charge: Tally::new(Owner::Misc),
        }
    }

    /// Start a replay operation
    pub fn start_replay(
        &self,
        store: Arc<EventStore>,
        request: StartReplayRequest,
    ) -> Result<StartReplayResponse> {
        let replay_id = Uuid::new_v4();
        let started_at = Utc::now();
        let config = request.config.unwrap_or_default();

        // Query events to replay
        let query = QueryEventsRequest {
            as_of_valid: None,
            since_valid: None,
            until_valid: None,
            entity_id: request.entity_id.clone(),
            event_type: request.event_type.clone(),
            tenant_id: None,
            as_of: request.to_timestamp,
            since: request.from_timestamp,
            until: request.to_timestamp,
            limit: None,
            event_type_prefix: None,
            exclude_event_type_prefix: None,
            payload_filter: None,
        };

        let events = store.query(&query)?;
        let total_events = events.len();

        tracing::info!(
            "🔄 Starting replay {} for {} events{}",
            replay_id,
            total_events,
            request
                .projection_name
                .as_ref()
                .map(|n| format!(" (projection: {n})"))
                .unwrap_or_default()
        );

        // Create replay state
        let state = ReplayState {
            id: replay_id,
            projection_name: request.projection_name.clone(),
            status: Arc::new(RwLock::new(ReplayStatus::Running)),
            started_at,
            completed_at: Arc::new(RwLock::new(None)),
            total_events,
            processed_events: Arc::new(AtomicU64::new(0)),
            failed_events: Arc::new(AtomicU64::new(0)),
            error_message: Arc::new(RwLock::new(None)),
            cancelled: Arc::new(AtomicBool::new(false)),
        };

        // Store replay state
        {
            let mut replays = self.replays.write();
            let before = replays.capacity();
            self.replays_charge.add(replay_heap(&state));
            replays.push(state);
            self.replays_charge
                .grow(before, replays.capacity(), size_of::<ReplayState>());
        }

        // Get the state references we need
        let replays = Arc::clone(&self.replays);
        let replay_idx = replays.read().len() - 1;

        // Spawn replay task
        tokio::spawn(async move {
            let result = Self::run_replay(
                store,
                events,
                request.projection_name,
                config,
                replays.clone(),
                replay_idx,
            )
            .await;

            // Update final status
            let mut replays_lock = replays.write();
            if let Some(state) = replays_lock.get_mut(replay_idx) {
                *state.completed_at.write() = Some(Utc::now());

                match result {
                    Ok(()) => {
                        if state.cancelled.load(Ordering::Relaxed) {
                            *state.status.write() = ReplayStatus::Cancelled;
                            tracing::info!("🛑 Replay {} cancelled", state.id);
                        } else {
                            *state.status.write() = ReplayStatus::Completed;
                            tracing::info!("✅ Replay {} completed successfully", state.id);
                        }
                    }
                    Err(e) => {
                        *state.status.write() = ReplayStatus::Failed;
                        *state.error_message.write() = Some(e.to_string());
                        tracing::error!("❌ Replay {} failed: {}", state.id, e);
                    }
                }
            }
        });

        Ok(StartReplayResponse {
            replay_id,
            status: ReplayStatus::Running,
            started_at,
            total_events,
        })
    }

    /// Internal replay execution
    // Kept `async`: this is the body of the `tokio::spawn`ed replay task and is
    // awaited as such. Rewriting it to return `impl Future` would mean threading
    // `std::future::ready(..)` through every early return in the replay loop —
    // a real edit to replay control flow to satisfy a style lint. It has no
    // `.await` only because the current batch loop is CPU-bound.
    #[allow(unknown_lints, clippy::unused_async_trait_impl)]
    async fn run_replay(
        store: Arc<EventStore>,
        events: Vec<Event>,
        projection_name: Option<String>,
        config: ReplayConfig,
        replays: Arc<RwLock<Vec<ReplayState>>>,
        replay_idx: usize,
    ) -> Result<()> {
        let total = events.len();
        let projections = store.projections.read();

        // Get target projection(s)
        let target_projections: Vec<(String, Arc<dyn Projection>)> =
            if let Some(name) = projection_name {
                if let Some(proj) = projections.get_projection(&name) {
                    vec![(name, proj)]
                } else {
                    return Err(AllSourceError::ValidationError(format!(
                        "Projection not found: {name}"
                    )));
                }
            } else {
                projections.list_projections()
            };

        drop(projections); // Release lock

        // Process events in batches
        for (batch_idx, chunk) in events.chunks(config.batch_size).enumerate() {
            // Check if cancelled
            {
                let replays_lock = replays.read();
                if let Some(state) = replays_lock.get(replay_idx)
                    && state.cancelled.load(Ordering::Relaxed)
                {
                    return Ok(());
                }
            }

            // Process batch
            for event in chunk {
                // Apply event to each target projection
                for (proj_name, projection) in &target_projections {
                    if let Err(e) = projection.process(event) {
                        tracing::warn!(
                            "Failed to process event {} in projection {}: {}",
                            event.id,
                            proj_name,
                            e
                        );

                        // Increment failed counter
                        let replays_lock = replays.read();
                        if let Some(state) = replays_lock.get(replay_idx) {
                            state.failed_events.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }

                // Increment processed counter
                let replays_lock = replays.read();
                if let Some(state) = replays_lock.get(replay_idx) {
                    let processed = state.processed_events.fetch_add(1, Ordering::Relaxed) + 1;

                    // Emit progress
                    if config.emit_progress && processed % config.progress_interval as u64 == 0 {
                        let progress = (processed as f64 / total as f64) * 100.0;
                        tracing::debug!(
                            "Replay progress: {}/{} ({:.1}%)",
                            processed,
                            total,
                            progress
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Get progress for a replay operation
    pub fn get_progress(&self, replay_id: Uuid) -> Result<ReplayProgress> {
        let replays = self.replays.read();

        let state = replays.iter().find(|r| r.id == replay_id).ok_or_else(|| {
            AllSourceError::ValidationError(format!("Replay not found: {replay_id}"))
        })?;

        let processed = state.processed_events.load(Ordering::Relaxed);
        let failed = state.failed_events.load(Ordering::Relaxed);
        let total_events = state.total_events;
        let started_at = state.started_at;
        let status = *state.status.read();
        let completed_at = *state.completed_at.read();
        let error_message = state.error_message.read().clone();

        drop(replays); // Release lock before calculations

        let progress_percentage = if total_events > 0 {
            (processed as f64 / total_events as f64) * 100.0
        } else {
            0.0
        };

        let updated_at = Utc::now();
        let elapsed_seconds = (updated_at - started_at).num_seconds().max(1) as f64;
        let events_per_second = processed as f64 / elapsed_seconds;

        Ok(ReplayProgress {
            replay_id,
            status,
            started_at,
            updated_at,
            completed_at,
            total_events,
            processed_events: processed as usize,
            failed_events: failed as usize,
            progress_percentage,
            events_per_second,
            error_message,
        })
    }

    /// Cancel a running replay
    pub fn cancel_replay(&self, replay_id: Uuid) -> Result<()> {
        let replays = self.replays.read();

        let state = replays.iter().find(|r| r.id == replay_id).ok_or_else(|| {
            AllSourceError::ValidationError(format!("Replay not found: {replay_id}"))
        })?;

        let status = *state.status.read();
        if status != ReplayStatus::Running {
            return Err(AllSourceError::ValidationError(format!(
                "Cannot cancel replay in status: {status:?}"
            )));
        }

        state.cancelled.store(true, Ordering::Relaxed);
        tracing::info!("🛑 Cancelling replay {}", replay_id);

        Ok(())
    }

    /// List all replay operations
    pub fn list_replays(&self) -> Vec<ReplayProgress> {
        let replays = self.replays.read();

        replays
            .iter()
            .map(|state| {
                let processed = state.processed_events.load(Ordering::Relaxed);
                let failed = state.failed_events.load(Ordering::Relaxed);
                let progress_percentage = if state.total_events > 0 {
                    (processed as f64 / state.total_events as f64) * 100.0
                } else {
                    0.0
                };

                let updated_at = Utc::now();
                let elapsed_seconds = (updated_at - state.started_at).num_seconds().max(1) as f64;
                let events_per_second = processed as f64 / elapsed_seconds;

                ReplayProgress {
                    replay_id: state.id,
                    status: *state.status.read(),
                    started_at: state.started_at,
                    updated_at,
                    completed_at: *state.completed_at.read(),
                    total_events: state.total_events,
                    processed_events: processed as usize,
                    failed_events: failed as usize,
                    progress_percentage,
                    events_per_second,
                    error_message: state.error_message.read().clone(),
                }
            })
            .collect()
    }

    /// Delete a completed or failed replay from history
    pub fn delete_replay(&self, replay_id: Uuid) -> Result<bool> {
        let mut replays = self.replays.write();

        let idx = replays
            .iter()
            .position(|r| r.id == replay_id)
            .ok_or_else(|| {
                AllSourceError::ValidationError(format!("Replay not found: {replay_id}"))
            })?;

        let status = *replays[idx].status.read();
        if status == ReplayStatus::Running {
            return Err(AllSourceError::ValidationError(
                "Cannot delete a running replay. Cancel it first.".to_string(),
            ));
        }

        let removed = replays.remove(idx);
        self.replays_charge.sub(replay_heap(&removed));
        tracing::info!("🗑️  Deleted replay {}", replay_id);

        Ok(true)
    }
}

impl Default for ReplayManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::Event;
    use serde_json::json;

    #[tokio::test]
    async fn test_replay_manager_creation() {
        let manager = ReplayManager::new();
        let replays = manager.list_replays();
        assert_eq!(replays.len(), 0);
    }

    #[tokio::test]
    async fn test_replay_progress_tracking() {
        let manager = ReplayManager::new();
        let store = Arc::new(EventStore::new());

        // Ingest some test events
        for i in 0..10 {
            let event = Event::from_strings(
                "test.event".to_string(),
                "test-entity".to_string(),
                "default".to_string(),
                json!({"value": i}),
                None,
            )
            .unwrap();
            store.ingest(&event).unwrap();
        }

        // Start replay
        let request = StartReplayRequest {
            projection_name: None,
            from_timestamp: None,
            to_timestamp: None,
            entity_id: None,
            event_type: None,
            config: Some(ReplayConfig {
                batch_size: 5,
                parallel: false,
                workers: 1,
                emit_progress: true,
                progress_interval: 5,
            }),
        };

        let response = manager.start_replay(store, request).unwrap();
        assert_eq!(response.status, ReplayStatus::Running);
        assert_eq!(response.total_events, 10);

        // Wait a bit for replay to process
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Check progress
        let progress = manager.get_progress(response.replay_id).unwrap();
        assert!(progress.processed_events <= 10);
    }
}
