use crate::{
    domain::{
        entities::Event,
        value_objects::{
            EntityId,
            system_stream::{
                SYSTEM_ENTITY_ID_PREFIX, SystemDomain, is_system_event_type,
                system_entity_id_value, system_event_type, system_tenant_id,
            },
        },
    },
    error::{AllSourceError, Result},
    infrastructure::{
        memory::ledger::{Owner, Tally},
        persistence::{
            system_checkpoint::{self, CHECKPOINT_DIR, FoldedStream, SystemCheckpoint},
            wal::{CoveredSegment, WALConfig, WALEntry, WriteAheadLog},
        },
    },
};
use chrono::{DateTime, Utc};
use parking_lot::{Mutex, RwLock};
use serde_json::Value as JsonValue;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

/// Events held in memory, each with its WAL sequence. A folded domain holds
/// only the events after the checkpoint the store booted from.
struct Resident {
    events: Vec<Event>,
    sequences: Vec<u64>,
    /// Events written per domain over the whole history, folded ones included.
    counts: BTreeMap<String, u64>,
    /// Serialized size of `events`, kept current on push and release so the
    /// memory sampler never walks the resident set under the lock.
    bytes: u64,
    /// `resident_bytes()` of `events`, charged to `system_store`.
    heap: Tally,
}

fn serialized_len(event: &Event) -> u64 {
    serde_json::to_vec(event).map_or(0, |bytes| bytes.len() as u64)
}

impl Default for Resident {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            sequences: Vec::new(),
            counts: BTreeMap::new(),
            bytes: 0,
            heap: Tally::new(Owner::SystemStore),
        }
    }
}

impl Resident {
    fn push(&mut self, sequence: u64, event: Event) {
        *self
            .counts
            .entry(domain_of(&event).to_string())
            .or_default() += 1;
        self.bytes += serialized_len(&event);
        self.heap.add(event.resident_bytes());
        self.sequences.push(sequence);
        self.events.push(event);
    }
}

fn domain_of(event: &Event) -> &str {
    let rest = event
        .entity_id_str()
        .strip_prefix(SYSTEM_ENTITY_ID_PREFIX)
        .unwrap_or_default();
    rest.split(':').next().unwrap_or(rest)
}

fn in_domain(event: &Event, domain: SystemDomain) -> bool {
    event
        .entity_id_str()
        .strip_prefix(SYSTEM_ENTITY_ID_PREFIX)
        .is_some_and(|rest| rest.starts_with(domain.as_str()))
}

/// What a system checkpoint needs from the store at one sequence.
pub struct CheckpointCapture {
    pub covered_seq: u64,
    pub segments: Vec<CoveredSegment>,
    pub domain_counts: BTreeMap<String, u64>,
    /// Every resident event outside `folded`.
    pub verbatim: Vec<(u64, Event)>,
    /// Resident events of `folded` with a sequence above the caller's base.
    pub tail: Vec<Event>,
}

/// How [`SystemMetadataStore::open`] treats checkpoints.
#[derive(Debug, Clone, Copy)]
pub struct SystemStoreOptions {
    /// Boot from the newest valid checkpoint; when false, replay the whole WAL.
    pub load_checkpoint: bool,
    /// Domains a checkpoint folds; their events before it are not resident.
    pub folded: &'static [SystemDomain],
}

impl Default for SystemStoreOptions {
    fn default() -> Self {
        Self {
            load_checkpoint: true,
            folded: &[SystemDomain::Tenant],
        }
    }
}

/// SystemMetadataStore provides durable storage for operational metadata.
///
/// It uses the same WAL-based durability model as the main EventStore but
/// writes to a separate directory for fault isolation. This means:
///
/// - A full disk on data storage does not prevent metadata writes
/// - System stream WAL can be backed up independently
/// - Different compaction/retention policies can apply
///
/// # Architecture
///
/// ```text
/// ┌──────────────────────────┐
/// │   SystemMetadataStore    │
/// │  ┌────────────────────┐  │
/// │  │  In-Memory Events  │  │  ← DashMap-backed read cache
/// │  │  (Vec<Event>)      │  │
/// │  └────────────────────┘  │
/// │  ┌────────────────────┐  │
/// │  │  WAL (fsync)       │  │  ← Durability guarantee
/// │  │  {system_dir}/wal  │  │
/// │  └────────────────────┘  │
/// └──────────────────────────┘
/// ```
pub struct SystemMetadataStore {
    resident: RwLock<Resident>,

    /// Held across the WAL append and the resident push, so a checkpoint
    /// capture never covers a sequence whose event is not resident yet.
    append_lock: Mutex<()>,

    /// Write-ahead log for durability
    wal: Arc<WriteAheadLog>,

    /// Base directory for system data
    data_dir: PathBuf,

    options: SystemStoreOptions,

    /// The checkpoint this store booted from.
    loaded: Option<SystemCheckpoint>,

    /// The sequence of the last event made resident. A capture refuses when
    /// the WAL has handed out a sequence beyond it.
    last_sequence: AtomicU64,

    /// Set by a failed WAL append. Its bytes may still reach the segment
    /// later without being resident, so no checkpoint may cover them until a
    /// restart reads the WAL again.
    append_failed: AtomicBool,
}

impl SystemMetadataStore {
    /// Create a new SystemMetadataStore with the given data directory.
    ///
    /// On creation, the store:
    /// 1. Initializes WAL in `{data_dir}/wal/`
    /// 2. Loads the newest valid checkpoint, if any
    /// 3. Recovers the WAL events the checkpoint does not cover
    pub fn new(data_dir: impl Into<PathBuf>) -> Result<Self> {
        Self::open(data_dir, SystemStoreOptions::default())
    }

    pub fn open(data_dir: impl Into<PathBuf>, options: SystemStoreOptions) -> Result<Self> {
        let data_dir = data_dir.into();
        let wal_dir = data_dir.join("wal");

        // Ensure directories exist
        std::fs::create_dir_all(&wal_dir).map_err(|e| {
            AllSourceError::StorageError(format!(
                "Failed to create system WAL directory {}: {}",
                wal_dir.display(),
                e
            ))
        })?;

        let wal_config = WALConfig {
            max_file_size: 16 * 1024 * 1024, // 16 MB (smaller than data WAL)
            sync_on_write: true,
            max_wal_files: 5,
            compress: false,
            ..WALConfig::default()
        };

        let wal = WriteAheadLog::new(&wal_dir, wal_config)?;
        let wal = Arc::new(wal);

        let mut resident = Resident::default();
        let checkpoint_dir = data_dir.join(CHECKPOINT_DIR);
        let mut loaded = options
            .load_checkpoint
            .then(|| system_checkpoint::load_latest(&checkpoint_dir))
            .flatten();

        let tail = match &loaded {
            Some(checkpoint) => {
                match wal.recover_for_append_after(checkpoint.covered_seq, &checkpoint.segments) {
                    Ok(tail) => Some(tail),
                    Err(e) => {
                        tracing::error!(
                            "System WAL contradicts checkpoint {}, replaying the whole WAL: {e}",
                            checkpoint.covered_seq
                        );
                        // Left in place, a stale checkpoint would win a later
                        // boot over everything this replay goes on to write.
                        match system_checkpoint::reject_all(&checkpoint_dir) {
                            Ok(moved) => tracing::error!(
                                "{moved} system checkpoints moved to {}/rejected",
                                checkpoint_dir.display()
                            ),
                            Err(e) => tracing::error!("System checkpoints not rejected: {e}"),
                        }
                        None
                    }
                }
            }
            None => None,
        };
        let recovered = if let Some(tail) = tail {
            let checkpoint = loaded.as_ref().expect("tail implies a checkpoint");
            for (sequence, event) in &checkpoint.verbatim {
                resident.sequences.push(*sequence);
                resident.bytes += serialized_len(event);
                resident.heap.add(event.resident_bytes());
                resident.events.push(event.clone());
            }
            resident.counts.clone_from(&checkpoint.domain_counts);
            tail
        } else {
            loaded = None;
            let floor = system_checkpoint::highest_named(&checkpoint_dir);
            wal.recover_entries_for_append(floor).map_err(|e| {
                tracing::error!("System WAL recovery failed: {}", e);
                AllSourceError::StorageError(format!("System WAL recovery failed: {e}"))
            })?
        };

        let mut skipped = 0usize;
        for WALEntry {
            sequence, event, ..
        } in recovered
        {
            if is_system_event_type(event.event_type_str()) {
                resident.push(sequence, event);
            } else {
                skipped += 1;
                tracing::warn!(
                    "Skipping non-system event in system WAL: {}",
                    event.event_type_str()
                );
            }
        }
        tracing::info!(
            "System store recovered: {} resident events, {} in history, checkpoint {:?}{}",
            resident.events.len(),
            resident.counts.values().sum::<u64>(),
            loaded.as_ref().map(|c| c.covered_seq),
            if skipped > 0 {
                format!(", {skipped} non-system events skipped")
            } else {
                String::new()
            }
        );

        let last_sequence = AtomicU64::new(wal.current_sequence());
        Ok(Self {
            resident: RwLock::new(resident),
            append_lock: Mutex::new(()),
            wal,
            data_dir,
            options,
            loaded,
            last_sequence,
            append_failed: AtomicBool::new(false),
        })
    }

    /// The folded state of `domain` in the checkpoint this store booted
    /// from. Resident events of `domain` all come after it.
    pub fn folded(&self, domain: SystemDomain) -> Option<&FoldedStream> {
        self.loaded.as_ref()?.folds.get(domain.as_str())
    }

    /// The checkpoint this store booted from.
    pub fn loaded_checkpoint(&self) -> Option<&SystemCheckpoint> {
        self.loaded.as_ref()
    }

    pub fn options(&self) -> SystemStoreOptions {
        self.options
    }

    pub fn checkpoint_dir(&self) -> PathBuf {
        self.data_dir.join(CHECKPOINT_DIR)
    }

    pub fn wal(&self) -> &WriteAheadLog {
        &self.wal
    }

    /// Drop the folded events a checkpoint at `covered_seq` holds. Call only
    /// once that checkpoint is durable, and under the checkpointer's base
    /// lock: `capture` reads resident events by index and must not see them
    /// move.
    pub fn release_folded(&self, covered_seq: u64) {
        let folded = self.options.folded;
        let mut resident = self.resident.write();
        let Resident {
            events,
            sequences,
            bytes,
            heap,
            ..
        } = &mut *resident;
        let keep: Vec<bool> = events
            .iter()
            .zip(sequences.iter())
            .map(|(event, sequence)| {
                *sequence > covered_seq || !folded.iter().any(|d| in_domain(event, *d))
            })
            .collect();
        let released: u64 = events
            .iter()
            .zip(keep.iter())
            .filter(|(_, kept)| !**kept)
            .map(|(event, _)| serialized_len(event))
            .sum();
        let released_heap: u64 = events
            .iter()
            .zip(keep.iter())
            .filter(|(_, kept)| !**kept)
            .map(|(event, _)| event.resident_bytes())
            .sum();
        heap.sub(released_heap);
        *bytes -= released;
        let mut mask = keep.iter().copied();
        events.retain(|_| mask.next().unwrap_or(true));
        let mut mask = keep.iter().copied();
        sequences.retain(|_| mask.next().unwrap_or(true));
        events.shrink_to_fit();
        sequences.shrink_to_fit();
    }

    /// Everything a checkpoint at the current sequence needs, read with
    /// appends stopped. `tail` holds the folded domains' events above
    /// `after_seq`.
    pub fn capture(&self, after_seq: u64) -> Result<CheckpointCapture> {
        let (covered_seq, segments, resident_len, domain_counts) = {
            let _appends = self.append_lock.lock();
            if self.append_failed.load(Ordering::SeqCst) {
                return Err(AllSourceError::StorageError(
                    "A system WAL append failed in this process; no checkpoint until restart"
                        .into(),
                ));
            }
            let covered_seq = self.wal.current_sequence();
            let last = self.last_sequence.load(Ordering::SeqCst);
            if covered_seq != last {
                return Err(AllSourceError::StorageError(format!(
                    "System WAL is at sequence {covered_seq} but the last resident event is {last}"
                )));
            }
            let segments = self.wal.covered_segments()?;
            let resident = self.resident.read();
            (
                covered_seq,
                segments,
                resident.events.len(),
                resident.counts.clone(),
            )
        };
        // Resident events are only ever appended, so the first `resident_len`
        // are exactly those at or below `covered_seq`; copying them in chunks
        // keeps appends waiting for one chunk at most.
        const CHUNK: usize = 8192;
        let folded = self.options.folded;
        let mut verbatim = Vec::new();
        let mut tail = Vec::new();
        for start in (0..resident_len).step_by(CHUNK) {
            let resident = self.resident.read();
            let end = (start + CHUNK).min(resident_len);
            for index in start..end {
                let (sequence, event) = (resident.sequences[index], &resident.events[index]);
                if folded.iter().any(|domain| in_domain(event, *domain)) {
                    if sequence > after_seq {
                        tail.push(event.clone());
                    }
                } else {
                    verbatim.push((sequence, event.clone()));
                }
            }
        }
        Ok(CheckpointCapture {
            covered_seq,
            segments,
            domain_counts,
            verbatim,
            tail,
        })
    }

    /// Every system event on disk of `domain`, in sequence order, read from
    /// the WAL whatever the checkpoint covers. For a fold that cannot use
    /// the checkpoint, and for verifying one.
    pub fn read_domain_history(&self, domain: SystemDomain) -> Result<Vec<(u64, Event)>> {
        Ok(self
            .wal
            .read_all_entries()?
            .into_iter()
            .map(|entry| (entry.sequence, entry.event))
            .filter(|(_, event)| in_domain(event, domain))
            .collect())
    }

    /// Append a system event to the store.
    ///
    /// The event is:
    /// 1. Validated as a system event
    /// 2. Written to WAL (fsync)
    /// 3. Added to in-memory cache
    pub fn append(&self, event: Event) -> Result<()> {
        // Validate: must be a system event
        if !is_system_event_type(event.event_type_str()) {
            return Err(AllSourceError::ValidationError(format!(
                "SystemMetadataStore only accepts _system.* event types, got '{}'",
                event.event_type_str()
            )));
        }

        let _appends = self.append_lock.lock();
        let sequence = self.wal.append(event.clone()).inspect_err(|e| {
            self.append_failed.store(true, Ordering::SeqCst);
            tracing::error!("System WAL append failed; checkpoints stop until restart: {e}");
        })?;
        self.resident.write().push(sequence, event);
        self.last_sequence.store(sequence, Ordering::SeqCst);

        Ok(())
    }

    /// Create and append a system event from components.
    pub fn append_system_event(
        &self,
        event_type_str: &str,
        entity_id: EntityId,
        payload: JsonValue,
        metadata: Option<JsonValue>,
    ) -> Result<Event> {
        let event_type = system_event_type(event_type_str);
        let tenant_id = system_tenant_id();

        let event = if let Some(metadata) = metadata {
            Event::with_metadata(event_type, entity_id, tenant_id, payload, metadata)
        } else {
            Event::new(event_type, entity_id, tenant_id, payload)
        };

        self.append(event.clone())?;
        Ok(event)
    }

    /// Read all resident events from a system stream (filtered by entity_id
    /// prefix). For a folded domain, only those after the loaded checkpoint.
    ///
    /// For example, `read_stream(SystemDomain::Tenant)` returns all events
    /// with entity_id starting with `_system:tenant:`.
    pub fn read_stream(&self, domain: SystemDomain) -> Vec<Event> {
        let prefix = format!("{}{}", SYSTEM_ENTITY_ID_PREFIX, domain.as_str());
        let resident = self.resident.read();
        resident
            .events
            .iter()
            .filter(|e| e.entity_id_str().starts_with(&prefix))
            .cloned()
            .collect()
    }

    /// Read all events for a specific entity in a system stream.
    ///
    /// For example, `read_entity(SystemDomain::Tenant, "acme-corp")` returns all
    /// events with entity_id `_system:tenant:acme-corp`.
    pub fn read_entity(&self, domain: SystemDomain, resource_id: &str) -> Vec<Event> {
        let entity_id = system_entity_id_value(domain, resource_id);
        let entity_id_str = entity_id.as_str();
        let resident = self.resident.read();
        resident
            .events
            .iter()
            .filter(|e| e.entity_id_str() == entity_id_str)
            .cloned()
            .collect()
    }

    /// Read events from a system stream starting from a given version (position).
    ///
    /// This is useful for incremental catch-up after initial bootstrap.
    pub fn read_stream_from(&self, domain: SystemDomain, from_version: usize) -> Vec<Event> {
        let prefix = format!("{}{}", SYSTEM_ENTITY_ID_PREFIX, domain.as_str());
        let resident = self.resident.read();
        resident
            .events
            .iter()
            .filter(|e| e.entity_id_str().starts_with(&prefix))
            .skip(from_version)
            .cloned()
            .collect()
    }

    /// Read all system events (across all domains).
    pub fn read_all(&self) -> Vec<Event> {
        self.resident.read().events.clone()
    }

    /// Events ever written to a system stream, including those a checkpoint
    /// folded.
    pub fn count_stream(&self, domain: SystemDomain) -> usize {
        self.resident
            .read()
            .counts
            .get(domain.as_str())
            .map_or(0, |count| *count as usize)
    }

    /// Events ever written to the system store, including those a checkpoint
    /// folded.
    pub fn total_events(&self) -> usize {
        self.resident.read().counts.values().sum::<u64>() as usize
    }

    /// Events held in memory.
    pub fn resident_events(&self) -> usize {
        self.resident.read().events.len()
    }

    /// Serialized size of the events held in memory. An estimate; process
    /// RSS is measured separately.
    pub fn resident_bytes(&self) -> u64 {
        self.resident.read().bytes
    }

    /// Get the data directory path.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Read events filtered by event type within a time range.
    pub fn query_by_type_and_time(
        &self,
        event_type_str: &str,
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Vec<Event> {
        let resident = self.resident.read();
        let mut results: Vec<Event> = resident
            .events
            .iter()
            .filter(|e| {
                if e.event_type_str() != event_type_str {
                    return false;
                }
                if let Some(start) = start
                    && e.timestamp() < start
                {
                    return false;
                }
                if let Some(end) = end
                    && e.timestamp() > end
                {
                    return false;
                }
                true
            })
            .cloned()
            .collect();

        results.sort_by_key(crate::domain::entities::event::Event::timestamp);

        if let Some(limit) = limit {
            results.truncate(limit);
        }

        results
    }
}

impl std::fmt::Debug for SystemMetadataStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemMetadataStore")
            .field("data_dir", &self.data_dir)
            .field("total_events", &self.total_events())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::value_objects::system_stream::{SystemDomain, tenant_events};
    use tempfile::TempDir;

    fn create_test_store() -> (SystemMetadataStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store = SystemMetadataStore::new(temp_dir.path().join("__system")).unwrap();
        (store, temp_dir)
    }

    #[test]
    fn test_create_system_store() {
        let (store, _dir) = create_test_store();
        assert_eq!(store.total_events(), 0);
    }

    #[test]
    fn test_append_system_event() {
        let (store, _dir) = create_test_store();

        let entity_id = system_entity_id_value(SystemDomain::Tenant, "acme");
        let event = store
            .append_system_event(
                tenant_events::CREATED,
                entity_id,
                serde_json::json!({"name": "ACME Corp", "quotas": {}}),
                None,
            )
            .unwrap();

        assert_eq!(store.total_events(), 1);
        assert_eq!(event.event_type_str(), "_system.tenant.created");
        assert_eq!(event.entity_id_str(), "_system:tenant:acme");
        assert_eq!(event.tenant_id_str(), "_system");
    }

    #[test]
    fn test_reject_non_system_event() {
        let (store, _dir) = create_test_store();

        let event = Event::from_strings(
            "order.placed".to_string(),
            "order-123".to_string(),
            "default".to_string(),
            serde_json::json!({}),
            None,
        )
        .unwrap();

        let result = store.append(event);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("only accepts _system.*")
        );
    }

    #[test]
    fn test_read_stream() {
        let (store, _dir) = create_test_store();

        // Add tenant events
        store
            .append_system_event(
                tenant_events::CREATED,
                system_entity_id_value(SystemDomain::Tenant, "acme"),
                serde_json::json!({"name": "ACME"}),
                None,
            )
            .unwrap();

        store
            .append_system_event(
                tenant_events::CREATED,
                system_entity_id_value(SystemDomain::Tenant, "beta"),
                serde_json::json!({"name": "Beta"}),
                None,
            )
            .unwrap();

        // Add config event (different domain)
        store
            .append_system_event(
                "_system.config.set",
                system_entity_id_value(SystemDomain::Config, "max_conn"),
                serde_json::json!({"value": 100}),
                None,
            )
            .unwrap();

        // Read tenant stream only
        let tenant_events = store.read_stream(SystemDomain::Tenant);
        assert_eq!(tenant_events.len(), 2);

        // Read config stream only
        let config_events = store.read_stream(SystemDomain::Config);
        assert_eq!(config_events.len(), 1);

        // Total
        assert_eq!(store.total_events(), 3);
    }

    #[test]
    fn resident_bytes_tracks_appends_and_release() {
        let (store, _dir) = create_test_store();
        store
            .append_system_event(
                tenant_events::CREATED,
                system_entity_id_value(SystemDomain::Tenant, "acme"),
                serde_json::json!({"name": "ACME"}),
                None,
            )
            .unwrap();
        store
            .append_system_event(
                "_system.config.set",
                system_entity_id_value(SystemDomain::Config, "max_conn"),
                serde_json::json!({"value": 100}),
                None,
            )
            .unwrap();
        let serialized_total = |store: &SystemMetadataStore| -> u64 {
            store
                .read_all()
                .iter()
                .map(|event| serde_json::to_vec(event).unwrap().len() as u64)
                .sum()
        };
        assert_eq!(store.resident_bytes(), serialized_total(&store));

        store.release_folded(u64::MAX);
        assert_eq!(store.resident_events(), 1);
        assert_eq!(store.resident_bytes(), serialized_total(&store));
    }

    #[test]
    fn test_read_entity() {
        let (store, _dir) = create_test_store();

        store
            .append_system_event(
                tenant_events::CREATED,
                system_entity_id_value(SystemDomain::Tenant, "acme"),
                serde_json::json!({"name": "ACME"}),
                None,
            )
            .unwrap();

        store
            .append_system_event(
                tenant_events::UPDATED,
                system_entity_id_value(SystemDomain::Tenant, "acme"),
                serde_json::json!({"name": "ACME Corp"}),
                None,
            )
            .unwrap();

        store
            .append_system_event(
                tenant_events::CREATED,
                system_entity_id_value(SystemDomain::Tenant, "other"),
                serde_json::json!({"name": "Other"}),
                None,
            )
            .unwrap();

        let acme_events = store.read_entity(SystemDomain::Tenant, "acme");
        assert_eq!(acme_events.len(), 2);
        assert_eq!(acme_events[0].event_type_str(), "_system.tenant.created");
        assert_eq!(acme_events[1].event_type_str(), "_system.tenant.updated");
    }

    #[test]
    fn test_read_stream_from_version() {
        let (store, _dir) = create_test_store();

        for i in 0..5 {
            store
                .append_system_event(
                    tenant_events::CREATED,
                    system_entity_id_value(SystemDomain::Tenant, &format!("t{i}")),
                    serde_json::json!({"index": i}),
                    None,
                )
                .unwrap();
        }

        // Read from version 3 (skip first 3)
        let events = store.read_stream_from(SystemDomain::Tenant, 3);
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_count_stream() {
        let (store, _dir) = create_test_store();

        store
            .append_system_event(
                tenant_events::CREATED,
                system_entity_id_value(SystemDomain::Tenant, "a"),
                serde_json::json!({}),
                None,
            )
            .unwrap();

        store
            .append_system_event(
                tenant_events::CREATED,
                system_entity_id_value(SystemDomain::Tenant, "b"),
                serde_json::json!({}),
                None,
            )
            .unwrap();

        assert_eq!(store.count_stream(SystemDomain::Tenant), 2);
        assert_eq!(store.count_stream(SystemDomain::Config), 0);
    }

    #[test]
    fn test_wal_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let system_dir = temp_dir.path().join("__system");

        // Write events
        {
            let store = SystemMetadataStore::new(&system_dir).unwrap();
            store
                .append_system_event(
                    tenant_events::CREATED,
                    system_entity_id_value(SystemDomain::Tenant, "acme"),
                    serde_json::json!({"name": "ACME"}),
                    None,
                )
                .unwrap();

            store
                .append_system_event(
                    "_system.config.set",
                    system_entity_id_value(SystemDomain::Config, "key1"),
                    serde_json::json!({"value": "hello"}),
                    None,
                )
                .unwrap();

            assert_eq!(store.total_events(), 2);
        }
        // Store dropped — simulates restart

        // Recover from WAL
        {
            let store = SystemMetadataStore::new(&system_dir).unwrap();
            assert_eq!(store.total_events(), 2);

            let tenant_events = store.read_stream(SystemDomain::Tenant);
            assert_eq!(tenant_events.len(), 1);
            assert_eq!(tenant_events[0].entity_id_str(), "_system:tenant:acme");

            let config_events = store.read_stream(SystemDomain::Config);
            assert_eq!(config_events.len(), 1);
        }
    }

    #[test]
    fn test_query_by_type_and_time() {
        let (store, _dir) = create_test_store();

        store
            .append_system_event(
                tenant_events::CREATED,
                system_entity_id_value(SystemDomain::Tenant, "a"),
                serde_json::json!({}),
                None,
            )
            .unwrap();

        store
            .append_system_event(
                tenant_events::UPDATED,
                system_entity_id_value(SystemDomain::Tenant, "a"),
                serde_json::json!({}),
                None,
            )
            .unwrap();

        let results = store.query_by_type_and_time(tenant_events::CREATED, None, None, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].event_type_str(), "_system.tenant.created");
    }

    #[test]
    fn test_debug_impl() {
        let (store, _dir) = create_test_store();
        let debug_str = format!("{store:?}");
        assert!(debug_str.contains("SystemMetadataStore"));
        assert!(debug_str.contains("total_events"));
    }
}
