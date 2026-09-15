#[cfg(feature = "server")]
use crate::application::services::webhook::WebhookRegistry;
#[cfg(feature = "server")]
use crate::infrastructure::observability::metrics::MetricsRegistry;
#[cfg(feature = "server")]
use crate::infrastructure::web::websocket::WebSocketManager;
use crate::{
    application::{
        dto::QueryEventsRequest,
        services::{
            consumer::ConsumerRegistry,
            exactly_once::{ExactlyOnceConfig, ExactlyOnceRegistry},
            pipeline::PipelineManager,
            projection::{EntitySnapshotProjection, EventCounterProjection, ProjectionManager},
            replay::ReplayManager,
            schema::{SchemaRegistry, SchemaRegistryConfig},
            schema_evolution::SchemaEvolutionManager,
        },
    },
    domain::entities::Event,
    error::{AllSourceError, Result},
    infrastructure::{
        persistence::{
            compaction::{CompactionConfig, CompactionManager},
            index::{EventIndex, IndexEntry},
            snapshot::{SnapshotConfig, SnapshotManager, SnapshotType},
            storage::ParquetStorage,
            tenant_loader::TenantLoader,
            wal::{WALConfig, WriteAheadLog},
        },
        query::geospatial::GeoIndex,
    },
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use parking_lot::RwLock;
use std::{path::PathBuf, sync::Arc};
#[cfg(feature = "server")]
use tokio::sync::mpsc;

/// High-performance event store with columnar storage
/// What a read credential is allowed to see, enforced inside the store.
///
/// A read key is otherwise all-or-nothing: "let this tool read the store" and
/// "let this tool read every credential ever issued" are the same grant. That is
/// not theoretical — auth events carry session tokens whose value IS the
/// `entity_id`, so a routine query rendered live bearer tokens into an agent's
/// context (#265). Redacting on write cannot fix it: the token is the lookup
/// key.
///
/// Deliberately **not** `Deserialize`. This must never be settable from a
/// request body, or a caller widens its own scope by omitting the field.
///
/// Allow-list, not deny-list, on purpose: a stream family added later is
/// excluded until someone names it, so the failure mode of forgetting is a
/// missing read rather than a leak.
#[derive(Debug, Clone, Default)]
pub struct ReadScope {
    allow_entity_prefixes: Option<Vec<String>>,
}

impl ReadScope {
    /// Everything in the tenant — the behaviour of every read before scopes.
    pub fn unrestricted() -> Self {
        Self {
            allow_entity_prefixes: None,
        }
    }

    /// Only entities whose id starts with one of `prefixes`.
    ///
    /// An empty list denies everything, which is the safe reading of "scoped to
    /// nothing" and stops an accidentally-empty config granting full access.
    pub fn allow_entity_prefixes<I, S>(prefixes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            allow_entity_prefixes: Some(prefixes.into_iter().map(Into::into).collect()),
        }
    }

    /// Whether this scope can see `entity_id`.
    pub fn permits(&self, entity_id: &str) -> bool {
        match &self.allow_entity_prefixes {
            None => true,
            Some(allowed) => allowed.iter().any(|p| entity_id.starts_with(p.as_str())),
        }
    }

    /// True when this scope restricts nothing.
    pub fn is_unrestricted(&self) -> bool {
        self.allow_entity_prefixes.is_none()
    }
}

pub struct EventStore {
    /// In-memory event storage
    events: Arc<RwLock<Vec<Event>>>,

    /// High-performance concurrent index
    index: Arc<EventIndex>,

    /// Projection manager for real-time aggregations
    pub(crate) projections: Arc<RwLock<ProjectionManager>>,

    /// Optional persistent storage (v0.2 feature)
    storage: Option<Arc<RwLock<ParquetStorage>>>,

    /// WebSocket manager for real-time event streaming (v0.2 feature)
    #[cfg(feature = "server")]
    websocket_manager: Arc<WebSocketManager>,

    /// Snapshot manager for fast state recovery (v0.2 feature)
    snapshot_manager: Arc<SnapshotManager>,

    /// Write-Ahead Log for durability (v0.2 feature)
    wal: Option<Arc<WriteAheadLog>>,

    /// Compaction manager for Parquet optimization (v0.2 feature)
    compaction_manager: Option<Arc<CompactionManager>>,

    /// Schema registry for event validation (v0.5 feature)
    schema_registry: Arc<SchemaRegistry>,

    /// Replay manager for event replay and projection rebuilding (v0.5 feature)
    replay_manager: Arc<ReplayManager>,

    /// Pipeline manager for stream processing (v0.5 feature)
    pipeline_manager: Arc<PipelineManager>,

    /// Prometheus metrics registry (v0.6 feature)
    #[cfg(feature = "server")]
    metrics: Arc<MetricsRegistry>,

    /// Total events ingested (for metrics)
    total_ingested: Arc<RwLock<u64>>,

    /// Projection state cache for Query Service integration (v0.7 feature)
    /// Key format: "{projection_name}:{entity_id}"
    /// This DashMap provides O(1) access with ~11.9 μs latency
    projection_state_cache: Arc<DashMap<String, serde_json::Value>>,

    /// Projection status overrides (v0.13 feature)
    /// Tracks pause/start state per projection name: "running" or "paused"
    projection_status: Arc<DashMap<String, String>>,

    /// Webhook registry for outbound event delivery (v0.11 feature)
    #[cfg(feature = "server")]
    webhook_registry: Arc<WebhookRegistry>,

    /// Channel sender for async webhook delivery tasks
    #[cfg(feature = "server")]
    webhook_tx: Arc<RwLock<Option<mpsc::UnboundedSender<WebhookDeliveryTask>>>>,

    /// Geospatial index for coordinate-based queries (v2.0 feature)
    geo_index: Arc<GeoIndex>,

    /// Exactly-once processing registry (v2.0 feature)
    exactly_once: Arc<ExactlyOnceRegistry>,

    /// Autonomous schema evolution manager (v2.0 feature)
    schema_evolution: Arc<SchemaEvolutionManager>,

    /// Per-entity version counters for optimistic concurrency control (v0.14 feature)
    /// Key: entity_id string, Value: monotonic version (number of events for that entity)
    entity_versions: Arc<DashMap<String, u64>>,

    /// Durable consumer registry for subscription cursor tracking (v0.14 feature)
    consumer_registry: Arc<ConsumerRegistry>,

    /// In-process broadcast of every successfully-ingested event. Always enabled
    /// so embedded consumers (TUI, web) can tail changes without the `server`
    /// feature / HTTP stack. Lagging receivers see `RecvError::Lagged`.
    event_broadcast_tx: tokio::sync::broadcast::Sender<Arc<Event>>,

    /// Per-tenant lazy-load bookkeeping. Tracks which tenants have
    /// been hydrated from Parquet into the in-memory pile and
    /// serializes concurrent first-loads of the same tenant. See
    /// `ensure_tenant_loaded` and Step 2 of the sustainable data
    /// strategy.
    tenant_loader: Arc<TenantLoader>,

    /// Cadence of the runtime checkpoint loop (Step 6). `None` means
    /// the loop is disabled — WAL grows until boot. Production reads
    /// this from `ALLSOURCE_CHECKPOINT_INTERVAL_SECONDS`. Stored on
    /// the store so background tasks can read it without re-parsing
    /// the env.
    checkpoint_interval_secs: Option<u64>,

    /// Read-only (replica) mode. Set when a second process attaches to a
    /// data-dir already owned by a live writer (see Prime's data-dir lock).
    /// A read-only store replays the WAL + Parquet into memory at boot so it
    /// can serve reads, but it MUST NOT truncate the WAL (that would unlink
    /// the inode the owner is still appending to — issue #201) and rejects
    /// all writes with `AllSourceError::ReadOnly`.
    read_only: bool,

    /// Held shared from an event's WAL append until it is in the Parquet
    /// batch, and exclusively while `checkpoint` seals the WAL. Without it an
    /// event can sit in a sealed segment yet miss the flush that retires it.
    durability_gate: RwLock<()>,
}

/// A task queued for async webhook delivery
#[cfg(feature = "server")]
#[derive(Debug, Clone)]
pub struct WebhookDeliveryTask {
    pub webhook: crate::application::services::webhook::WebhookSubscription,
    pub event: Event,
}

/// Reduce `items` to the `offset .. offset + limit` window under `order`,
/// leaving that window sorted and everything past it dropped.
///
/// `limit == None` means "no window": the whole slice is sorted. Otherwise the
/// window is partitioned off with `select_nth_unstable_by` — O(N) — and only
/// the `offset + limit` retained items are sorted, O(k log k), instead of
/// sorting all N (issue #251). `order` MUST be a total order (no two distinct
/// items compare `Equal`); with one, this is observationally identical to
/// sorting everything and then windowing, ties included.
///
/// The first `offset` items are kept, not skipped — the caller drops them,
/// which is what makes `has_more`/`total` independent of the window.
fn select_window<T>(
    items: &mut Vec<T>,
    offset: usize,
    limit: Option<usize>,
    order: impl Fn(&T, &T) -> std::cmp::Ordering + Copy,
) {
    match limit.map(|limit| offset.saturating_add(limit)) {
        Some(0) => {
            items.clear();
            return;
        }
        Some(window_end) if window_end < items.len() => {
            items.select_nth_unstable_by(window_end - 1, order);
            items.truncate(window_end);
        }
        // No limit, or a window that already covers everything: nothing to
        // partition off, the sort below orders the whole set.
        _ => {}
    }
    items.sort_unstable_by(order);
}

impl EventStore {
    /// Create a new in-memory event store
    pub fn new() -> Self {
        Self::with_config(EventStoreConfig::default())
    }

    /// Create event store with custom configuration
    pub fn with_config(config: EventStoreConfig) -> Self {
        let mut projections = ProjectionManager::new();

        // Register built-in projections
        projections.register(Arc::new(EntitySnapshotProjection::new("entity_snapshots")));
        projections.register(Arc::new(EventCounterProjection::new("event_counters")));

        // Initialize persistent storage if configured
        let storage = config
            .storage_dir
            .as_ref()
            .and_then(|dir| match ParquetStorage::new(dir) {
                Ok(storage) => {
                    tracing::info!("✅ Parquet persistence enabled at: {}", dir.display());
                    Some(Arc::new(RwLock::new(storage)))
                }
                Err(e) => {
                    tracing::error!("❌ Failed to initialize Parquet storage: {}", e);
                    None
                }
            });

        // Initialize WAL if configured (v0.2 feature)
        let wal = config.wal_dir.as_ref().and_then(|dir| {
            match WriteAheadLog::new(dir, config.wal_config.clone()) {
                Ok(wal) => {
                    tracing::info!("✅ WAL enabled at: {}", dir.display());
                    Some(Arc::new(wal))
                }
                Err(e) => {
                    tracing::error!("❌ Failed to initialize WAL: {}", e);
                    None
                }
            }
        });

        // Initialize compaction manager if Parquet storage is enabled (v0.2 feature)
        let compaction_manager = config.storage_dir.as_ref().map(|dir| {
            let manager = CompactionManager::new(dir, config.compaction_config.clone());
            Arc::new(manager)
        });

        // Initialize schema registry (v0.5 feature)
        let schema_registry = Arc::new(SchemaRegistry::new(config.schema_registry_config.clone()));
        tracing::info!("✅ Schema registry enabled");

        // Initialize replay manager (v0.5 feature)
        let replay_manager = Arc::new(ReplayManager::new());
        tracing::info!("✅ Replay manager enabled");

        // Initialize pipeline manager (v0.5 feature)
        let pipeline_manager = Arc::new(PipelineManager::new());
        tracing::info!("✅ Pipeline manager enabled");

        // Initialize metrics registry (v0.6 feature)
        #[cfg(feature = "server")]
        let metrics = {
            let m = MetricsRegistry::new();
            tracing::info!("✅ Prometheus metrics registry initialized");
            m
        };

        // Initialize projection state cache (v0.7 feature)
        let projection_state_cache = Arc::new(DashMap::new());
        tracing::info!("✅ Projection state cache initialized");

        // Initialize webhook registry (v0.11 feature)
        #[cfg(feature = "server")]
        let webhook_registry = {
            let w = Arc::new(WebhookRegistry::new());
            tracing::info!("✅ Webhook registry initialized");
            w
        };

        // Unconditional in-process event broadcaster so embedded consumers
        // (TUI, web) can live-reload without the `server` feature.
        let (event_broadcast_tx, _) = tokio::sync::broadcast::channel(1024);

        let store = Self {
            events: Arc::new(RwLock::new(Vec::new())),
            index: Arc::new(EventIndex::new()),
            projections: Arc::new(RwLock::new(projections)),
            storage,
            #[cfg(feature = "server")]
            websocket_manager: Arc::new(WebSocketManager::new()),
            snapshot_manager: Arc::new(SnapshotManager::new(config.snapshot_config)),
            wal,
            compaction_manager,
            schema_registry,
            replay_manager,
            pipeline_manager,
            #[cfg(feature = "server")]
            metrics,
            total_ingested: Arc::new(RwLock::new(0)),
            projection_state_cache,
            projection_status: Arc::new(DashMap::new()),
            #[cfg(feature = "server")]
            webhook_registry,
            #[cfg(feature = "server")]
            webhook_tx: Arc::new(RwLock::new(None)),
            geo_index: Arc::new(GeoIndex::new()),
            exactly_once: Arc::new(ExactlyOnceRegistry::new(ExactlyOnceConfig::default())),
            schema_evolution: Arc::new(SchemaEvolutionManager::new()),
            entity_versions: Arc::new(DashMap::new()),
            consumer_registry: Arc::new(ConsumerRegistry::new()),
            event_broadcast_tx,
            tenant_loader: {
                let loader = TenantLoader::new();
                if let Some(budget) = config.cache_byte_budget {
                    loader.set_byte_budget(budget);
                    tracing::info!(
                        "✅ Cache byte budget set to {} bytes ({:.2} GiB) — LRU eviction enabled",
                        budget,
                        budget as f64 / (1024.0 * 1024.0 * 1024.0)
                    );
                } else {
                    tracing::info!(
                        "✅ Cache budget unset — every loaded tenant stays resident \
                         (set ALLSOURCE_CACHE_BYTES to enable eviction)"
                    );
                }
                Arc::new(loader)
            },
            checkpoint_interval_secs: config.checkpoint_interval_secs,
            read_only: config.read_only,
            durability_gate: RwLock::new(()),
        };

        if config.read_only {
            tracing::info!(
                "📖 EventStore opened READ-ONLY (replica): WAL will be replayed for reads but \
                 not truncated; writes are rejected"
            );
        }

        // Boot is now O(1) regardless of dataset size (Step 2 of the
        // sustainable data strategy). Pre-Step-2 we scanned every
        // Parquet file at startup; on the production volume that
        // grew past available memory and Core OOM'd during recovery
        // (issue #160). Now:
        //
        //   - Parquet data stays on disk. Tenants are hydrated on
        //     demand by `ensure_tenant_loaded`, called from the
        //     query path on first access.
        //   - WAL is still recovered eagerly. WAL is bounded
        //     (rotates / truncates after each Parquet flush), so
        //     replaying it is O(recent un-flushed writes) — small
        //     by construction and required for correctness, since
        //     those events aren't durably in Parquet yet.
        //
        // After WAL recovery we still checkpoint recovered events
        // to Parquet and truncate the WAL. The lazy-load path's
        // dedupe in `append_loaded_event` (index.get_by_id check)
        // makes it safe for the same event to be reachable through
        // both WAL recovery and a subsequent ensure_tenant_loaded
        // pass on the same tenant.
        if let Some(ref wal) = store.wal {
            match wal.recover() {
                Ok(recovered_events) if !recovered_events.is_empty() => {
                    let mut wal_new = 0usize;
                    for event in recovered_events {
                        let offset = store.events.read().len();
                        if let Err(e) = store.index.index_event(
                            event.id,
                            event.entity_id_str(),
                            event.event_type_str(),
                            event.timestamp,
                            offset,
                        ) {
                            tracing::error!("Failed to re-index WAL event {}: {}", event.id, e);
                        }

                        if let Err(e) = store.projections.read().process_event(&event) {
                            tracing::error!("Failed to re-process WAL event {}: {}", event.id, e);
                        }

                        *store
                            .entity_versions
                            .entry(event.entity_id_str().to_string())
                            .or_insert(0) += 1;

                        store.events.write().push(event);
                        wal_new += 1;
                    }

                    // Step 6: expose replay size as a gauge so ops
                    // can graph "how big was the last replay?" and
                    // catch regressions where the checkpoint loop
                    // stops draining the WAL.
                    #[cfg(feature = "server")]
                    store.metrics.wal_replay_events_total.set(wal_new as i64);

                    if wal_new > 0 {
                        let total = store.events.read().len();
                        // total_ingested now reflects "events the
                        // process knows about" rather than "events
                        // ever written" — Parquet data isn't loaded
                        // on boot, so the count grows as tenants
                        // get hydrated. Consumers that need the
                        // historical total should look at Parquet
                        // file stats, not this counter.
                        *store.total_ingested.write() = total as u64;
                        tracing::info!(
                            "✅ Recovered {} events from WAL (Parquet data stays cold until \
                             first per-tenant query)",
                            wal_new
                        );

                        // Checkpoint WAL events to Parquet — buffer
                        // them into the per-tenant Parquet batches
                        // first, then flush. Without this,
                        // flush_storage() finds an empty current
                        // batch and silently no-ops, the WAL gets
                        // truncated, and the events exist only in
                        // memory (lost on next restart).
                        //
                        // Skip entirely in read-only (replica) mode: a replica
                        // does not own the WAL. `wal.truncate()` unlinks the WAL
                        // file, which would delete the inode the owning writer is
                        // still appending to and silently lose its in-flight
                        // writes (issue #201). The replica keeps the recovered
                        // events in memory for reads and leaves the file alone.
                        if let Some(ref storage) = store.storage
                            && !config.read_only
                        {
                            tracing::info!(
                                "📸 Checkpointing {} WAL events to Parquet storage...",
                                wal_new
                            );
                            let parquet = storage.read();
                            let events = store.events.read();
                            let mut buffered = 0usize;
                            for event in events.iter().skip(events.len() - wal_new) {
                                if let Err(e) = parquet.append_event(event.clone()) {
                                    tracing::error!(
                                        "Failed to buffer WAL event for Parquet: {}",
                                        e
                                    );
                                } else {
                                    buffered += 1;
                                }
                            }
                            drop(events);
                            drop(parquet);

                            if buffered < wal_new {
                                tracing::error!(
                                    "Buffered {} of {} recovered WAL events; leaving the WAL \
                                     in place so none are lost",
                                    buffered,
                                    wal_new
                                );
                            } else if buffered > 0 {
                                if let Err(e) = store.flush_storage() {
                                    tracing::error!("Failed to checkpoint to Parquet: {}", e);
                                } else if let Err(e) = wal.truncate() {
                                    tracing::error!(
                                        "Failed to truncate WAL after checkpoint: {}",
                                        e
                                    );
                                } else {
                                    tracing::info!(
                                        "✅ WAL checkpointed and truncated ({} events)",
                                        buffered
                                    );
                                }
                            }
                        }
                    }
                }
                Ok(_) => {
                    tracing::debug!("No events to recover from WAL");
                    #[cfg(feature = "server")]
                    store.metrics.wal_replay_events_total.set(0);
                }
                Err(e) => {
                    tracing::error!("❌ WAL recovery failed: {}", e);
                }
            }
        } else if store.storage.is_some() {
            tracing::info!(
                "📂 Boot complete (lazy-load mode): Parquet data stays on disk until first \
                 per-tenant query"
            );
        }

        store
    }

    /// Whether this store was opened read-only (replica mode).
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Return `Err(AllSourceError::ReadOnly)` if the store is a read-only
    /// replica. Called at the top of every write path so a replica never
    /// appends to a WAL it does not own.
    fn ensure_writable(&self) -> Result<()> {
        if self.read_only {
            return Err(crate::error::AllSourceError::ReadOnly(
                "this AllSource instance is a read-only replica — the data directory is owned by \
                 another running process. Stop the other process, or run a single shared writer \
                 (e.g. Prime in --mode http) and point clients at it."
                    .to_string(),
            ));
        }
        Ok(())
    }

    /// Ingest a new event with optional optimistic concurrency check.
    ///
    /// If `expected_version` is `Some(v)`, the write is rejected with
    /// `VersionConflict` unless the entity's current version equals `v`.
    /// The version check and WAL append are atomic (locked together).
    ///
    /// Returns the new entity version after the append.
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn ingest_with_expected_version(
        &self,
        event: &Event,
        expected_version: Option<u64>,
    ) -> Result<u64> {
        // Reject writes in read-only (replica) mode before touching the WAL.
        self.ensure_writable()?;

        // Validate event first (before any locking)
        self.validate_event(event)?;

        let entity_id = event.entity_id_str().to_string();
        let _durable = self.durability_gate.read();

        // Atomic version check + append: hold the DashMap entry lock
        // to prevent TOCTOU races between check and write.
        let new_version = {
            let mut version_entry = self.entity_versions.entry(entity_id.clone()).or_insert(0);
            let current = *version_entry;

            if let Some(expected) = expected_version
                && current != expected
            {
                return Err(crate::error::AllSourceError::VersionConflict { expected, current });
            }

            // Write to WAL FIRST for durability (under version lock to keep atomicity)
            if let Some(ref wal) = self.wal {
                wal.append(event.clone())?;
            }

            *version_entry += 1;
            *version_entry
        };

        // From here on, the event is durable (WAL) and version is bumped.
        // Continue with indexing, projections, storage, and broadcast.
        self.ingest_post_wal(event)?;

        Ok(new_version)
    }

    /// Post-WAL ingestion: index, projections, storage, broadcast.
    /// Called after WAL append and version bump are complete.
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    fn ingest_post_wal(&self, event: &Event) -> Result<()> {
        #[cfg(feature = "server")]
        let timer = self.metrics.ingestion_duration_seconds.start_timer();

        let mut events = self.events.write();
        let offset = events.len();

        // Index the event
        self.index.index_event(
            event.id,
            event.entity_id_str(),
            event.event_type_str(),
            event.timestamp,
            offset,
        )?;

        // Process through projections
        let projections = self.projections.read();
        projections.process_event(event)?;
        drop(projections);

        // Process through pipelines
        let pipeline_results = self.pipeline_manager.process_event(event);
        if !pipeline_results.is_empty() {
            tracing::debug!(
                "Event {} processed by {} pipeline(s)",
                event.id,
                pipeline_results.len()
            );
            for (pipeline_id, result) in pipeline_results {
                tracing::trace!("Pipeline {} result: {:?}", pipeline_id, result);
            }
        }

        // Persist to Parquet storage if enabled
        if let Some(ref storage) = self.storage {
            let storage = storage.read();
            storage.append_event(event.clone())?;
        }

        // Store the event in memory
        events.push(event.clone());
        let total_events = events.len();
        drop(events);

        // Broadcast to in-process subscribers (always on) + optional WS.
        let event_arc = Arc::new(event.clone());
        let _ = self.event_broadcast_tx.send(Arc::clone(&event_arc));
        #[cfg(feature = "server")]
        self.websocket_manager.broadcast_event(event_arc);

        // Dispatch to matching webhook subscriptions
        #[cfg(feature = "server")]
        self.dispatch_webhooks(event);

        // Update geospatial index
        self.geo_index.index_event(event);

        // Autonomous schema evolution
        self.schema_evolution
            .analyze_event(event.event_type_str(), &event.payload);

        // Check if automatic snapshot should be created
        self.check_auto_snapshot(event.entity_id_str(), event);

        // Update metrics
        #[cfg(feature = "server")]
        {
            self.metrics.events_ingested_total.inc();
            self.metrics
                .events_ingested_by_type
                .with_label_values(&[event.event_type_str()])
                .inc();
            self.metrics.storage_events_total.set(total_events as i64);
        }

        // Update legacy total counter
        let mut total = self.total_ingested.write();
        *total += 1;

        #[cfg(feature = "server")]
        timer.observe_duration();

        tracing::debug!("Event ingested: {} (offset: {})", event.id, offset);

        Ok(())
    }

    /// Ingest a new event into the store
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn ingest(&self, event: &Event) -> Result<()> {
        // Start metrics timer (v0.6 feature)
        #[cfg(feature = "server")]
        let timer = self.metrics.ingestion_duration_seconds.start_timer();

        // Reject writes in read-only (replica) mode before touching the WAL.
        if let Err(e) = self.ensure_writable() {
            #[cfg(feature = "server")]
            {
                self.metrics.ingestion_errors_total.inc();
                timer.observe_duration();
            }
            return Err(e);
        }

        // Validate event
        let validation_result = self.validate_event(event);
        if let Err(e) = validation_result {
            #[cfg(feature = "server")]
            {
                self.metrics.ingestion_errors_total.inc();
                timer.observe_duration();
            }
            return Err(e);
        }

        let _durable = self.durability_gate.read();

        // Write to WAL FIRST for durability (v0.2 feature)
        // This ensures event is persisted before processing
        if let Some(ref wal) = self.wal
            && let Err(e) = wal.append(event.clone())
        {
            #[cfg(feature = "server")]
            {
                self.metrics.ingestion_errors_total.inc();
                timer.observe_duration();
            }
            return Err(e);
        }

        // Track per-entity version (unconditional increment, no version check)
        *self
            .entity_versions
            .entry(event.entity_id_str().to_string())
            .or_insert(0) += 1;

        let mut events = self.events.write();
        let offset = events.len();

        // Index the event
        self.index.index_event(
            event.id,
            event.entity_id_str(),
            event.event_type_str(),
            event.timestamp,
            offset,
        )?;

        // Process through projections
        let projections = self.projections.read();
        projections.process_event(event)?;
        drop(projections); // Release lock

        // Process through pipelines (v0.5 feature)
        // Pipelines can transform, filter, and aggregate events in real-time
        let pipeline_results = self.pipeline_manager.process_event(event);
        if !pipeline_results.is_empty() {
            tracing::debug!(
                "Event {} processed by {} pipeline(s)",
                event.id,
                pipeline_results.len()
            );
            // Pipeline results could be stored, emitted, or forwarded elsewhere
            // For now, we just log them for observability
            for (pipeline_id, result) in pipeline_results {
                tracing::trace!("Pipeline {} result: {:?}", pipeline_id, result);
            }
        }

        // Persist to Parquet storage if enabled (v0.2)
        if let Some(ref storage) = self.storage {
            let storage = storage.read();
            storage.append_event(event.clone())?;
        }

        // Store the event in memory
        events.push(event.clone());
        let total_events = events.len();
        drop(events); // Release lock early

        // Broadcast to in-process subscribers + optional WS.
        let event_arc = Arc::new(event.clone());
        let _ = self.event_broadcast_tx.send(Arc::clone(&event_arc));
        #[cfg(feature = "server")]
        self.websocket_manager.broadcast_event(event_arc);

        // Dispatch to matching webhook subscriptions (v0.11 feature)
        #[cfg(feature = "server")]
        self.dispatch_webhooks(event);

        // Update geospatial index (v2.0 feature)
        self.geo_index.index_event(event);

        // Autonomous schema evolution (v2.0 feature)
        self.schema_evolution
            .analyze_event(event.event_type_str(), &event.payload);

        // Check if automatic snapshot should be created (v0.2 feature)
        self.check_auto_snapshot(event.entity_id_str(), event);

        // Update metrics (v0.6 feature)
        #[cfg(feature = "server")]
        {
            self.metrics.events_ingested_total.inc();
            self.metrics
                .events_ingested_by_type
                .with_label_values(&[event.event_type_str()])
                .inc();
            self.metrics.storage_events_total.set(total_events as i64);
        }

        // Update legacy total counter
        let mut total = self.total_ingested.write();
        *total += 1;

        #[cfg(feature = "server")]
        timer.observe_duration();

        tracing::debug!("Event ingested: {} (offset: {})", event.id, offset);

        Ok(())
    }

    /// Ingest a batch of events with a single write lock acquisition.
    ///
    /// All events are validated first. If any event fails validation, no
    /// events are stored (all-or-nothing validation). Events are then written
    /// to WAL, indexed, processed through projections, and pushed to the
    /// events vector under a single write lock.
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn ingest_batch(&self, batch: Vec<Event>) -> Result<()> {
        if batch.is_empty() {
            return Ok(());
        }

        // Reject writes in read-only (replica) mode before touching the WAL.
        self.ensure_writable()?;

        // Phase 1: Validate all events before acquiring any locks
        for event in &batch {
            self.validate_event(event)?;
        }

        let _durable = self.durability_gate.read();

        // Phase 2: Write all events to WAL (before write lock, for durability)
        if let Some(ref wal) = self.wal {
            for event in &batch {
                wal.append(event.clone())?;
            }
        }

        // Phase 3: Single write lock for index + projections + push
        let mut events = self.events.write();
        let projections = self.projections.read();

        for event in batch {
            let offset = events.len();

            self.index.index_event(
                event.id,
                event.entity_id_str(),
                event.event_type_str(),
                event.timestamp,
                offset,
            )?;

            projections.process_event(&event)?;
            self.pipeline_manager.process_event(&event);

            if let Some(ref storage) = self.storage {
                let storage = storage.read();
                storage.append_event(event.clone())?;
            }

            self.geo_index.index_event(&event);
            self.schema_evolution
                .analyze_event(event.event_type_str(), &event.payload);

            // Track per-entity version
            *self
                .entity_versions
                .entry(event.entity_id_str().to_string())
                .or_insert(0) += 1;

            // Broadcast to in-process subscribers
            let _ = self.event_broadcast_tx.send(Arc::new(event.clone()));

            events.push(event);
        }

        let total_events = events.len();
        drop(projections);
        drop(events);

        let mut total = self.total_ingested.write();
        *total += total_events as u64;

        Ok(())
    }

    /// Ingest a replicated event from the leader (follower mode).
    ///
    /// Unlike `ingest()`, this method:
    /// - Skips WAL writing (the follower's WalReceiver manages its own local WAL)
    /// - Skips schema validation (the leader already validated)
    /// - Still indexes, processes projections/pipelines, and broadcasts to WebSocket clients
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn ingest_replicated(&self, event: &Event) -> Result<()> {
        #[cfg(feature = "server")]
        let timer = self.metrics.ingestion_duration_seconds.start_timer();

        let mut events = self.events.write();
        let offset = events.len();

        // Index the event
        self.index.index_event(
            event.id,
            event.entity_id_str(),
            event.event_type_str(),
            event.timestamp,
            offset,
        )?;

        // Process through projections
        let projections = self.projections.read();
        projections.process_event(event)?;
        drop(projections);

        // Process through pipelines
        let pipeline_results = self.pipeline_manager.process_event(event);
        if !pipeline_results.is_empty() {
            tracing::debug!(
                "Replicated event {} processed by {} pipeline(s)",
                event.id,
                pipeline_results.len()
            );
        }

        // Track per-entity version
        *self
            .entity_versions
            .entry(event.entity_id_str().to_string())
            .or_insert(0) += 1;

        // Store the event in memory
        events.push(event.clone());
        let total_events = events.len();
        drop(events);

        // Broadcast to in-process subscribers + optional WS.
        let event_arc = Arc::new(event.clone());
        let _ = self.event_broadcast_tx.send(Arc::clone(&event_arc));
        #[cfg(feature = "server")]
        self.websocket_manager.broadcast_event(event_arc);

        // Update metrics
        #[cfg(feature = "server")]
        {
            self.metrics.events_ingested_total.inc();
            self.metrics
                .events_ingested_by_type
                .with_label_values(&[event.event_type_str()])
                .inc();
            self.metrics.storage_events_total.set(total_events as i64);
        }

        let mut total = self.total_ingested.write();
        *total += 1;

        #[cfg(feature = "server")]
        timer.observe_duration();

        tracing::debug!(
            "Replicated event ingested: {} (offset: {})",
            event.id,
            offset
        );

        Ok(())
    }

    /// Get the current version for an entity (number of events appended for it).
    /// Returns 0 if the entity has no events.
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn get_entity_version(&self, entity_id: &str) -> u64 {
        self.entity_versions.get(entity_id).map_or(0, |v| *v)
    }

    /// Get the consumer registry for durable subscriptions.
    pub fn consumer_registry(&self) -> &ConsumerRegistry {
        &self.consumer_registry
    }

    /// Subscribe to every successfully-ingested event in this store.
    ///
    /// Returns a `tokio::sync::broadcast::Receiver` that yields an `Arc<Event>`
    /// for each ingest. Always available — does not require the `server`
    /// feature. Lagging receivers surface `RecvError::Lagged`.
    pub fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<Arc<Event>> {
        self.event_broadcast_tx.subscribe()
    }

    /// Replace the default in-memory consumer registry with a durable one.
    ///
    /// Called during startup when system repositories are available, so that
    /// consumer cursors survive Core restarts via WAL persistence.
    pub fn set_consumer_registry(&mut self, registry: Arc<ConsumerRegistry>) {
        self.consumer_registry = registry;
    }

    /// Get the total number of events in the store (used as max offset for consumer ack).
    pub fn total_events(&self) -> usize {
        self.events.read().len()
    }

    /// Get events after a given offset, optionally filtered by event type prefixes.
    /// Used by consumer polling to fetch unprocessed events.
    pub fn events_after_offset(
        &self,
        offset: u64,
        filters: &[String],
        limit: usize,
    ) -> Vec<(u64, Event)> {
        let events = self.events.read();
        let start = offset as usize;
        if start >= events.len() {
            return vec![];
        }

        events[start..]
            .iter()
            .enumerate()
            .filter(|(_, event)| ConsumerRegistry::matches_filters(event.event_type_str(), filters))
            .take(limit)
            .map(|(i, event)| ((start + i + 1) as u64, event.clone()))
            .collect()
    }

    /// Get the WebSocket manager for this store
    #[cfg(feature = "server")]
    pub fn websocket_manager(&self) -> Arc<WebSocketManager> {
        Arc::clone(&self.websocket_manager)
    }

    /// Get the snapshot manager for this store
    pub fn snapshot_manager(&self) -> Arc<SnapshotManager> {
        Arc::clone(&self.snapshot_manager)
    }

    /// Get the compaction manager for this store
    pub fn compaction_manager(&self) -> Option<Arc<CompactionManager>> {
        self.compaction_manager.as_ref().map(Arc::clone)
    }

    /// Get the schema registry for this store (v0.5 feature)
    pub fn schema_registry(&self) -> Arc<SchemaRegistry> {
        Arc::clone(&self.schema_registry)
    }

    /// Get the replay manager for this store (v0.5 feature)
    pub fn replay_manager(&self) -> Arc<ReplayManager> {
        Arc::clone(&self.replay_manager)
    }

    /// Get the pipeline manager for this store (v0.5 feature)
    pub fn pipeline_manager(&self) -> Arc<PipelineManager> {
        Arc::clone(&self.pipeline_manager)
    }

    /// Get the metrics registry for this store (v0.6 feature)
    #[cfg(feature = "server")]
    pub fn metrics(&self) -> Arc<MetricsRegistry> {
        Arc::clone(&self.metrics)
    }

    /// Get the projection manager for this store (v0.7 feature)
    pub fn projection_manager(&self) -> parking_lot::RwLockReadGuard<'_, ProjectionManager> {
        self.projections.read()
    }

    /// Register a custom projection at runtime.
    ///
    /// The projection will receive all future events via `process()`.
    /// Historical events are **not** replayed — only events ingested after
    /// registration will be processed by this projection.
    ///
    /// See [`register_projection_with_backfill`](Self::register_projection_with_backfill)
    /// to also process historical events.
    pub fn register_projection(
        &self,
        projection: Arc<dyn crate::application::services::projection::Projection>,
    ) {
        let mut pm = self.projections.write();
        pm.register(projection);
    }

    /// Register a custom projection and replay all existing events through it.
    ///
    /// After registration, the projection will also receive all future events.
    /// Historical events are replayed under a read lock — the projection's
    /// internal state (typically DashMap) handles concurrent access.
    ///
    /// Replay is ordered by `(timestamp, version)`. The in-memory pile can be
    /// physically out of order when Parquet is hydrated after WAL recovery —
    /// the WAL tail holds the newest events while Parquet holds the older
    /// history — so the backfill must sort before replaying. Projections with
    /// last-write-wins merge semantics produce wrong state otherwise.
    pub fn register_projection_with_backfill(
        &self,
        projection: &Arc<dyn crate::application::services::projection::Projection>,
    ) -> Result<()> {
        // First register so future events are processed
        {
            let mut pm = self.projections.write();
            pm.register(Arc::clone(projection));
        }

        // Then replay existing events in chronological order under read lock
        let events = self.events.read();
        let mut ordered: Vec<&Event> = events.iter().collect();
        ordered.sort_by(|a, b| {
            a.timestamp
                .cmp(&b.timestamp)
                .then_with(|| a.version.cmp(&b.version))
        });
        for event in ordered {
            projection.process(event)?;
        }

        Ok(())
    }

    /// Eagerly reconstruct the in-memory event pile from the full Parquet
    /// archive.
    ///
    /// The default boot path (Step 2, issue #160) keeps Parquet cold and
    /// hydrates tenants lazily on first query — the multi-tenant server
    /// cannot fit every tenant in memory. Embedded single-store consumers
    /// like Prime are the opposite case: their projections *are* the
    /// queryable surface and never trigger the lazy query path, so the
    /// projections must be backfilled from the complete history.
    ///
    /// Call this *before* registering projections. The dedupe in
    /// `append_loaded_event` makes it safe to run after WAL recovery —
    /// events already replayed from the WAL are not double-counted.
    /// No-op (and `Ok(0)`) when no Parquet storage is configured, e.g.
    /// in-memory test mode.
    ///
    /// Returns the number of events newly loaded from Parquet.
    pub fn hydrate_all_from_storage(&self) -> Result<usize> {
        let Some(storage) = self.storage.as_ref().map(Arc::clone) else {
            return Ok(0);
        };

        let events = storage.read().load_all_events()?;
        let read_count = events.len();
        let before = self.events.read().len();
        for event in events {
            self.append_loaded_event(event);
        }
        let applied = self.events.read().len() - before;

        // The pile is now the authoritative full history.
        *self.total_ingested.write() = self.events.read().len() as u64;

        tracing::info!(
            read = read_count,
            applied = applied,
            "🔄 hydrate_all_from_storage: in-memory pile reconstructed from Parquet"
        );
        Ok(applied)
    }

    /// Get the projection state cache for this store (v0.7 feature)
    /// Used by Elixir Query Service for state synchronization
    pub fn projection_state_cache(&self) -> Arc<DashMap<String, serde_json::Value>> {
        Arc::clone(&self.projection_state_cache)
    }

    /// Get the projection status map (v0.13 feature)
    pub fn projection_status(&self) -> Arc<DashMap<String, String>> {
        Arc::clone(&self.projection_status)
    }

    /// Get the webhook registry for this store (v0.11 feature)
    /// Geospatial index for coordinate-based queries (v2.0 feature)
    pub fn geo_index(&self) -> Arc<GeoIndex> {
        self.geo_index.clone()
    }

    /// Exactly-once processing registry (v2.0 feature)
    pub fn exactly_once(&self) -> Arc<ExactlyOnceRegistry> {
        self.exactly_once.clone()
    }

    /// Schema evolution manager (v2.0 feature)
    pub fn schema_evolution(&self) -> Arc<SchemaEvolutionManager> {
        self.schema_evolution.clone()
    }

    /// Get a read-locked snapshot of all events (for EventQL/GraphQL queries).
    ///
    /// Returns an `Arc` reference to the internal events vec, avoiding a full
    /// clone. The caller holds a read lock for the duration of the `Arc`
    /// lifetime — prefer short-lived usage.
    pub fn snapshot_events(&self) -> Vec<Event> {
        self.events.read().clone()
    }

    /// Compact token events for an entity by replacing matching events with a
    /// single merged event. Used by the embedded streaming feature.
    ///
    /// Returns `Ok(true)` if compaction was performed, `Ok(false)` if no
    /// matching events were found.
    ///
    /// **Note:** The merged event is processed through projections *without*
    /// clearing the removed events' projection state first. Projections that
    /// accumulate state (e.g., counters) should be designed to handle this
    /// (the merged event replaces individual tokens, not adds to them).
    ///
    /// **Crash safety:** The WAL append happens *after* the in-memory swap
    /// under the write lock. If the process crashes before the WAL write,
    /// no change is persisted — WAL replay restores the pre-compaction state.
    /// If the process crashes after the WAL write, replay sees the merged
    /// event (and the original tokens, which are idempotent to replay since
    /// the merged event supersedes them).
    ///
    /// The write lock is held for the swap + WAL write + index rebuild.
    /// The index rebuild is O(N) over all events, which is acceptable for
    /// embedded workloads but should not be called in hot paths for large stores.
    pub fn compact_entity_tokens(
        &self,
        entity_id: &str,
        token_event_type: &str,
        merged_event: Event,
    ) -> Result<bool> {
        // Reject writes in read-only (replica) mode before touching the WAL.
        self.ensure_writable()?;

        // Phase 1: Read-only check — do we have anything to compact?
        {
            let events = self.events.read();
            let has_tokens = events
                .iter()
                .any(|e| e.entity_id_str() == entity_id && e.event_type_str() == token_event_type);
            if !has_tokens {
                return Ok(false);
            }
        }

        // Phase 2: Process merged event through projections (no write lock held)
        let projections = self.projections.read();
        projections.process_event(&merged_event)?;
        drop(projections);

        // Phase 3: Acquire write lock for the swap + WAL + index rebuild
        let mut events = self.events.write();

        events.retain(|e| {
            !(e.entity_id_str() == entity_id && e.event_type_str() == token_event_type)
        });

        events.push(merged_event.clone());

        // WAL append inside write lock: crash before this line = no change persisted.
        // Crash after = merged event in WAL, original tokens also in WAL but
        // superseded by the merged event's entity_id + event_type.
        if let Some(ref wal) = self.wal {
            wal.append(merged_event)?;
        }

        // Rebuild entire index since retain() shifted event positions.
        // Errors here indicate a corrupt event (missing entity_id/event_type)
        // which should not happen for well-formed events. Log and continue
        // rather than failing the entire compaction.
        self.index.clear();
        for (offset, event) in events.iter().enumerate() {
            if let Err(e) = self.index.index_event(
                event.id,
                event.entity_id_str(),
                event.event_type_str(),
                event.timestamp,
                offset,
            ) {
                tracing::warn!(
                    event_id = %event.id,
                    offset,
                    "Failed to re-index event during compaction: {e}"
                );
            }
        }

        Ok(true)
    }

    #[cfg(feature = "server")]
    pub fn webhook_registry(&self) -> Arc<WebhookRegistry> {
        Arc::clone(&self.webhook_registry)
    }

    /// Set the channel for async webhook delivery.
    /// Called during server startup to wire the delivery worker.
    #[cfg(feature = "server")]
    pub fn set_webhook_tx(&self, tx: mpsc::UnboundedSender<WebhookDeliveryTask>) {
        *self.webhook_tx.write() = Some(tx);
        tracing::info!("Webhook delivery channel connected");
    }

    /// Dispatch matching webhooks for a given event (non-blocking).
    #[cfg(feature = "server")]
    fn dispatch_webhooks(&self, event: &Event) {
        let matching = self.webhook_registry.find_matching(event);
        if matching.is_empty() {
            return;
        }

        let tx_guard = self.webhook_tx.read();
        if let Some(ref tx) = *tx_guard {
            for webhook in matching {
                let task = WebhookDeliveryTask {
                    webhook,
                    event: event.clone(),
                };
                if let Err(e) = tx.send(task) {
                    tracing::warn!("Failed to queue webhook delivery: {}", e);
                }
            }
        }
    }

    /// Manually flush any pending events to persistent storage
    pub fn flush_storage(&self) -> Result<()> {
        if let Some(ref storage) = self.storage {
            let storage = storage.read();
            storage.flush()?;
            tracing::info!("✅ Flushed events to persistent storage");
        }
        Ok(())
    }

    /// Run a checkpoint: flush pending Parquet batches, then truncate the
    /// WAL through the checkpoint point (Step 6 of the sustainable data
    /// strategy).
    ///
    /// Order matters. We flush Parquet first; only on success do we
    /// truncate the WAL. If the process crashes between the flush and the
    /// truncate, the WAL still contains the events that were just durably
    /// written, and recovery will replay them. The dedupe in
    /// `append_loaded_event` (index probe) makes that idempotent — the
    /// event is already in Parquet, so the lazy-load splice no-ops once
    /// the tenant is hydrated.
    ///
    /// The reverse order would be unsafe: a crash between truncate and
    /// flush would lose committed events.
    ///
    /// This bounds dirty-restart replay time to one checkpoint interval
    /// regardless of total dataset size — that's the load-bearing
    /// property for cold-start time as ingest rate grows.
    ///
    /// No-op when no WAL is configured (in-memory-only mode).
    pub fn checkpoint(&self) -> Result<()> {
        let Some(ref wal) = self.wal else {
            // No WAL (in-memory-only mode). Still refresh storage-size metrics in
            // case Parquet-only persistence is configured.
            #[cfg(feature = "server")]
            self.refresh_storage_metrics();
            return Ok(());
        };

        if self.read_only {
            return Ok(());
        }

        // Seal under the gate so every event in a sealed segment has already
        // reached the Parquet batch this flush drains. Events that arrive
        // after the seal land in the active segment, which is kept.
        let active = {
            let _sealing = self.durability_gate.write();
            wal.seal()?
        };
        self.flush_storage()?;
        wal.remove_sealed(&active)?;
        tracing::debug!("✅ Checkpoint complete: Parquet flushed, sealed WAL segments retired");

        // Recompute on-disk storage size now that Parquet is flushed and the WAL
        // is truncated — the gauge reflects the post-checkpoint footprint.
        #[cfg(feature = "server")]
        self.refresh_storage_metrics();

        Ok(())
    }

    /// Public entrypoint to populate the on-disk storage gauges once, e.g. at boot
    /// so the dashboard's "storage" card is correct before the first checkpoint
    /// tick (and even when the checkpoint loop is disabled). Delegates to the
    /// internal refresh. No-op without persistent storage.
    #[cfg(feature = "server")]
    pub fn refresh_storage_metrics_now(&self) {
        self.refresh_storage_metrics();
    }

    /// Recompute the on-disk storage gauges from the real Parquet + WAL files and
    /// publish them to Prometheus: `allsource_storage_size_bytes` (Parquet bytes +
    /// WAL segment bytes), `allsource_parquet_files_total`, and
    /// `allsource_wal_segments_total`.
    ///
    /// These gauges were registered but never set, so they read a constant 0 — the
    /// dashboard's "storage" card therefore showed `—`. This is the population.
    ///
    /// HONESTY: this is a **platform/process-wide** figure — the size of the whole
    /// data directory across all tenants, not any single tenant's storage. It is
    /// surfaced as a platform metric and must not be presented as a tenant number.
    ///
    /// Called from the checkpoint loop (default every 60s), not the ingest hot
    /// path: it does one `statx` per Parquet file + per WAL segment. Best-effort —
    /// a stat error logs and leaves the previous gauge value in place rather than
    /// resetting it to a misleading 0.
    #[cfg(feature = "server")]
    fn refresh_storage_metrics(&self) {
        let Some(ref storage) = self.storage else {
            return;
        };

        let parquet_stats = match storage.read().stats() {
            Ok(stats) => stats,
            Err(e) => {
                tracing::warn!("storage-size metric refresh: failed to stat Parquet: {e}");
                return;
            }
        };

        let (wal_bytes, wal_segments) = match self.wal.as_ref() {
            Some(wal) => match wal.on_disk_stats() {
                Ok(stats) => stats,
                Err(e) => {
                    tracing::warn!("storage-size metric refresh: failed to stat WAL: {e}");
                    (0, 0)
                }
            },
            None => (0, 0),
        };

        let total_bytes = parquet_stats.total_size_bytes + wal_bytes;

        self.metrics
            .storage_size_bytes
            .set(total_bytes.min(i64::MAX as u64) as i64);
        self.metrics
            .parquet_files_total
            .set(parquet_stats.total_files as i64);
        self.metrics.wal_segments_total.set(wal_segments as i64);

        tracing::debug!(
            "storage-size metrics refreshed: {} bytes total ({} Parquet files, {} WAL segments)",
            total_bytes,
            parquet_stats.total_files,
            wal_segments
        );
    }

    /// Get the configured checkpoint cadence (used by background tasks).
    pub fn checkpoint_interval(&self) -> Option<std::time::Duration> {
        self.checkpoint_interval_secs
            .map(std::time::Duration::from_secs)
    }

    /// Hydrate `tenant_id`'s persisted Parquet data into the in-memory
    /// pile if it isn't already loaded. Cheap on the warm path
    /// (DashMap probe); on the cold path it walks just that tenant's
    /// subtree (`load_events_for_tenant`) and splices the events into
    /// `events`/`index`/`projections`/`entity_versions`.
    ///
    /// Concurrent first-callers for the same tenant serialize on a
    /// per-tenant Mutex (singleflight) so the disk read happens once.
    /// Other tenants are unaffected — distinct lock per tenant.
    ///
    /// Returns `Err` if the tenant_id fails the path-safety
    /// whitelist, the Parquet read fails, or another in-flight load
    /// holds the lock past the configured timeout. The caller (a
    /// query handler) is expected to surface that as a 5xx — see
    /// Step 2's "no infinite hangs" acceptance criterion.
    ///
    /// On failure, `loaded` is NOT marked, so a transient error is
    /// retried on the next request rather than poisoning the tenant
    /// permanently. A future commit may add a circuit breaker if
    /// thrash becomes an issue.
    ///
    /// No-op (and Ok) when no Parquet storage is configured — the
    /// in-memory-only mode used by tests has nothing to hydrate.
    pub fn ensure_tenant_loaded(&self, tenant_id: &str) -> Result<()> {
        // Fast path: warm tenant. Avoids the Mutex altogether.
        if self.tenant_loader.is_loaded(tenant_id) {
            return Ok(());
        }

        let Some(storage) = self.storage.as_ref().map(Arc::clone) else {
            // No persistent storage to load from. Mark loaded so we
            // don't keep re-entering the slow path.
            self.tenant_loader.mark_loaded(tenant_id);
            return Ok(());
        };

        // Singleflight: get-or-insert the per-tenant lock and try to
        // acquire it within the timeout budget.
        let lock = self.tenant_loader.lock_for(tenant_id);
        let timeout = self.tenant_loader.load_timeout();
        let _guard = lock.try_lock_for(timeout).ok_or_else(|| {
            AllSourceError::StorageError(format!(
                "ensure_tenant_loaded timed out after {timeout:?} waiting for in-flight load of \
                 tenant {tenant_id:?}"
            ))
        })?;

        // Re-check inside the lock — another thread may have completed
        // the load while we were waiting.
        if self.tenant_loader.is_loaded(tenant_id) {
            return Ok(());
        }

        let started = std::time::Instant::now();
        let events = storage.read().load_events_for_tenant(tenant_id)?;
        let read_count = events.len();

        let before = self.events.read().len();
        for event in events {
            self.append_loaded_event(event);
        }
        let applied = self.events.read().len() - before;

        // total_ingested only counts events newly added to memory.
        // Dedupe (e.g. WAL events re-checkpointed to Parquet) makes
        // applied < read_count possible.
        *self.total_ingested.write() += applied as u64;
        self.tenant_loader.mark_loaded(tenant_id);

        tracing::info!(
            tenant_id = tenant_id,
            read = read_count,
            applied = applied,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "ensure_tenant_loaded: tenant hydrated"
        );

        // Budget check (Step 3 #3). After splicing the new
        // tenant in, evict LRU tenants until we're back under
        // budget. Excludes the just-loaded tenant from the
        // candidate set — otherwise a single oversized tenant
        // would evict itself in a tight loop. If no other tenant
        // is loaded, accept the over-budget state and log a
        // warning so ops can see it.
        self.enforce_cache_budget(tenant_id);

        // Refresh the resident-bytes gauge — covers both the
        // load-with-no-eviction case and the post-eviction state.
        #[cfg(feature = "server")]
        self.metrics
            .cache_bytes
            .set(self.tenant_loader.total_bytes() as i64);

        Ok(())
    }

    /// Walks the LRU until total resident bytes are within the
    /// configured budget, calling `evict_tenant` on each victim.
    /// Excludes `recently_touched` from the candidate set so we
    /// don't evict the tenant that just triggered the call. Step 3
    /// #3 entry point.
    ///
    /// No-op when no budget is set or when already under budget —
    /// most queries take the early-return fast path. Eviction is
    /// the cold path; with a well-sized budget this only fires
    /// during warm-up of new tenants past the working-set size.
    fn enforce_cache_budget(&self, recently_touched: &str) {
        if !self.tenant_loader.over_budget() {
            return;
        }
        loop {
            let Some(victim) = self.tenant_loader.pick_lru_excluding(recently_touched) else {
                tracing::warn!(
                    cache_bytes = self.tenant_loader.total_bytes(),
                    budget = self.tenant_loader.byte_budget(),
                    recently_touched = recently_touched,
                    "cache over budget but no other tenant available to evict — \
                     a single tenant exceeds the budget; consider raising it"
                );
                return;
            };
            self.evict_tenant(&victim);
            if !self.tenant_loader.over_budget() {
                return;
            }
        }
    }

    /// True iff `ensure_tenant_loaded` has previously succeeded for
    /// this tenant. Diagnostic / testing API.
    pub fn is_tenant_loaded(&self, tenant_id: &str) -> bool {
        self.tenant_loader.is_loaded(tenant_id)
    }

    /// Drop `tenant_id` from the in-memory cache. Step 3 #2 of the
    /// sustainable data strategy.
    ///
    /// Removes every event for this tenant from the events Vec,
    /// rebuilds the index/entity_versions for the retained events
    /// (Vec offsets shift on remove, so the index has to be
    /// rebuilt), and resets the tenant_loader bookkeeping so a
    /// subsequent query triggers a fresh `ensure_tenant_loaded`.
    ///
    /// **Parquet is canonical, in-memory is just cache.** This
    /// only affects the in-memory side. Disk data is untouched —
    /// that's why eviction is safe even for tenants with
    /// recently-ingested data: a query after eviction transparently
    /// re-reads from Parquet.
    ///
    /// Projection state is NOT rolled back. Projections accumulate
    /// across boots and tenants (their durability story is
    /// separate); subtracting them would need replay support that
    /// doesn't exist in this commit. After eviction + re-load,
    /// projections may double-count the re-loaded events. Step 3
    /// #4's stress test only asserts the cache budget is held; a
    /// future commit will tackle projection-aware eviction.
    ///
    /// Locking: takes the events write lock for the full duration
    /// of the filter + re-index. Concurrent ingest blocks until
    /// done. Eviction is the cold path; the working set should
    /// stay in budget so this rarely fires.
    pub fn evict_tenant(&self, tenant_id: &str) {
        let mut events = self.events.write();
        let before = events.len();
        let evicted_bytes = self.tenant_loader.bytes_for(tenant_id);

        events.retain(|e| e.tenant_id_str() != tenant_id);
        let after = events.len();
        let dropped = before - after;

        if dropped == 0 {
            // Tenant had no events in memory. Still clear loader
            // state (e.g. a "loaded with zero events" marker) so
            // is_tenant_loaded reports the right thing.
            drop(events);
            self.tenant_loader.mark_unloaded(tenant_id);
            return;
        }

        // Rebuild the index — Vec offsets shifted under retain().
        // Rebuild entity_versions from scratch too, since the
        // counter reflects "how many events of this entity remain".
        self.index.clear();
        self.entity_versions.clear();
        for (offset, event) in events.iter().enumerate() {
            if let Err(e) = self.index.index_event(
                event.id,
                event.entity_id_str(),
                event.event_type_str(),
                event.timestamp,
                offset,
            ) {
                tracing::error!(
                    "Failed to re-index event during eviction of {}: {}",
                    tenant_id,
                    e
                );
            }
            *self
                .entity_versions
                .entry(event.entity_id_str().to_string())
                .or_insert(0) += 1;
        }
        drop(events);

        self.tenant_loader.mark_unloaded(tenant_id);

        // total_ingested under Steps 2-3 means "events currently
        // resident in memory". Subtract what we just dropped.
        let mut t = self.total_ingested.write();
        *t = t.saturating_sub(dropped as u64);
        drop(t);

        // Step 3 #4: cache observability. Increment the eviction
        // counter, refresh the resident-bytes gauge.
        #[cfg(feature = "server")]
        {
            self.metrics.cache_evictions_total.inc();
            self.metrics
                .cache_bytes
                .set(self.tenant_loader.total_bytes() as i64);
        }

        tracing::info!(
            tenant_id = tenant_id,
            events_dropped = dropped,
            bytes_freed = evicted_bytes,
            "evicted tenant from memory cache"
        );
    }

    /// Approximate resident bytes a single tenant occupies in the
    /// in-memory cache. Step 3 budget-tracking input. 0 for cold
    /// or evicted tenants.
    pub fn tenant_resident_bytes(&self, tenant_id: &str) -> u64 {
        self.tenant_loader.bytes_for(tenant_id)
    }

    /// Sum of resident-byte estimates across every loaded tenant.
    /// What the budget check compares against.
    pub fn cache_resident_bytes(&self) -> u64 {
        self.tenant_loader.total_bytes()
    }

    /// Splice a single loaded event into the in-memory structures
    /// (events vec, index, projections, entity_versions) atomically
    /// w.r.t. concurrent ingest. Used by `ensure_tenant_loaded`.
    ///
    /// The WAL recovery path on boot has its own (single-threaded)
    /// variant inline because boot can't race with ingest. This
    /// helper is the variant safe to call while traffic is flowing
    /// — it holds the events write lock across the index/offset
    /// assignment so (offset, push) stays atomic.
    ///
    /// Dedupes against events already in memory by event ID. Two
    /// paths can surface the same event:
    /// 1. WAL recovery on boot pushed it into memory.
    /// 2. The event was then checkpointed to Parquet and the
    ///    WAL truncated. A later ensure_tenant_loaded re-reads
    ///    the Parquet file, including this event.
    ///
    /// Without the dedupe, step 2 would double-count the event.
    /// The check is O(1) — DashMap probe by UUID — and the
    /// alternative (loading every tenant before truncating WAL)
    /// would defeat the lazy-load.
    fn append_loaded_event(&self, event: Event) {
        if self.index.get_by_id(&event.id).is_some() {
            return;
        }

        let event_bytes = event.estimated_size_bytes();
        let tenant = event.tenant_id_str().to_string();

        let mut events = self.events.write();
        let offset = events.len();

        if let Err(e) = self.index.index_event(
            event.id,
            event.entity_id_str(),
            event.event_type_str(),
            event.timestamp,
            offset,
        ) {
            tracing::error!("Failed to index loaded event {}: {}", event.id, e);
        }

        if let Err(e) = self.projections.read().process_event(&event) {
            tracing::error!("Failed to project loaded event {}: {}", event.id, e);
        }

        *self
            .entity_versions
            .entry(event.entity_id_str().to_string())
            .or_insert(0) += 1;

        events.push(event);
        // Account for the bytes AFTER the push so a panic in the
        // index/projection path doesn't leave the counter inflated.
        // The DashMap update is itself the last fallible step.
        self.tenant_loader.add_bytes(&tenant, event_bytes);
    }

    /// Manually create a snapshot for an entity
    pub fn create_snapshot(&self, entity_id: &str) -> Result<()> {
        // Get all events for this entity
        let events = self.query(&QueryEventsRequest {
            entity_id: Some(entity_id.to_string()),
            event_type: None,
            tenant_id: None,
            as_of: None,
            since: None,
            until: None,
            limit: None,
            event_type_prefix: None,
            exclude_event_type_prefix: None,
            payload_filter: None,
        })?;

        if events.is_empty() {
            return Err(AllSourceError::EntityNotFound(entity_id.to_string()));
        }

        // Build current state
        let mut state = serde_json::json!({});
        for event in &events {
            if let serde_json::Value::Object(ref mut state_map) = state
                && let serde_json::Value::Object(ref payload_map) = event.payload
            {
                for (key, value) in payload_map {
                    state_map.insert(key.clone(), value.clone());
                }
            }
        }

        let last_event = events.last().unwrap();
        self.snapshot_manager.create_snapshot(
            entity_id,
            state,
            last_event.timestamp,
            events.len(),
            SnapshotType::Manual,
        )?;

        Ok(())
    }

    /// Check and create automatic snapshots if needed
    fn check_auto_snapshot(&self, entity_id: &str, event: &Event) {
        // Count events for this entity
        let entity_event_count = self
            .index
            .get_by_entity(entity_id)
            .map_or(0, |entries| entries.len());

        if self.snapshot_manager.should_create_snapshot(
            entity_id,
            entity_event_count,
            event.timestamp,
        ) {
            // Create snapshot in background (don't block ingestion)
            if let Err(e) = self.create_snapshot(entity_id) {
                tracing::warn!(
                    "Failed to create automatic snapshot for {}: {}",
                    entity_id,
                    e
                );
            }
        }
    }

    /// Validate an event before ingestion
    fn validate_event(&self, event: &Event) -> Result<()> {
        // EntityId and EventType value objects already validate non-empty in their constructors
        // So these checks are now redundant, but we keep them for explicit validation
        if event.entity_id_str().is_empty() {
            return Err(AllSourceError::ValidationError(
                "entity_id cannot be empty".to_string(),
            ));
        }

        if event.event_type_str().is_empty() {
            return Err(AllSourceError::ValidationError(
                "event_type cannot be empty".to_string(),
            ));
        }

        // Reject system namespace events from user-facing ingestion.
        // System events are written exclusively via SystemMetadataStore.
        if event.event_type().is_system() {
            return Err(AllSourceError::ValidationError(
                "Event types starting with '_system.' are reserved for internal use".to_string(),
            ));
        }

        Ok(())
    }

    /// Reset a projection by clearing its state and reprocessing all events
    pub fn reset_projection(&self, name: &str) -> Result<usize> {
        let projection_manager = self.projections.read();
        let projection = projection_manager.get_projection(name).ok_or_else(|| {
            AllSourceError::EntityNotFound(format!("Projection '{name}' not found"))
        })?;

        // Clear existing state
        projection.clear();

        // Clear cached state for this projection
        let prefix = format!("{name}:");
        let keys_to_remove: Vec<String> = self
            .projection_state_cache
            .iter()
            .filter(|entry| entry.key().starts_with(&prefix))
            .map(|entry| entry.key().clone())
            .collect();
        for key in keys_to_remove {
            self.projection_state_cache.remove(&key);
        }

        // Reprocess all events through this projection
        let events = self.events.read();
        let mut reprocessed = 0usize;
        for event in events.iter() {
            if projection.process(event).is_ok() {
                reprocessed += 1;
            }
        }

        Ok(reprocessed)
    }

    /// Get a single event by its UUID
    pub fn get_event_by_id(&self, event_id: &uuid::Uuid) -> Result<Option<Event>> {
        if let Some(offset) = self.index.get_by_id(event_id) {
            let events = self.events.read();
            Ok(events.get(offset).cloned())
        } else {
            Ok(None)
        }
    }

    /// Query events based on filters (optimized with indices).
    ///
    /// Ascending by `(timestamp, version)`, windowed by `request.limit`.
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn query(&self, request: &QueryEventsRequest) -> Result<Vec<Event>> {
        self.query_window(request, 0, false)
            .map(|(events, _)| events)
    }

    /// [`Self::query`] under a read scope the request body cannot widen.
    pub fn query_scoped(
        &self,
        request: &QueryEventsRequest,
        scope: &ReadScope,
    ) -> Result<Vec<Event>> {
        self.query_window_scoped(request, 0, false, scope)
            .map(|(events, _)| events)
    }

    /// Query events and return only the requested window, plus the total number
    /// of matches the window was taken from.
    ///
    /// `request.limit` bounds the window; `offset` skips that many matches
    /// first; `descending` returns newest first. The returned total is the
    /// pre-window match count, which is what `total_count`/`has_more` need.
    ///
    /// Cost (issue #251). Matching still costs an O(N) index scan over the
    /// entity's/type's entries — that is inherent to reporting `total`, and this
    /// method does not pretend otherwise. What `limit` *does* bound is the
    /// ordering and the materialization: matches are held as borrowed
    /// references, the `offset + limit` window is partitioned off with
    /// `select_nth_unstable_by` (O(N)) and only that window is sorted
    /// (O(k log k)) and cloned. So a `limit=1` read over a 100k-event entity
    /// scans 100k index entries but sorts and clones one event, where it
    /// previously sorted and cloned 100k.
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn query_window(
        &self,
        request: &QueryEventsRequest,
        offset: usize,
        descending: bool,
    ) -> Result<(Vec<Event>, usize)> {
        self.query_window_scoped(request, offset, descending, &ReadScope::unrestricted())
    }

    /// [`Self::query_window`] under a [`ReadScope`].
    ///
    /// Every read path in this store funnels through here, which is the point:
    /// a scope enforced in one API surface leaves the others open, and the
    /// surfaces that matter (HTTP, the embedded API, MCP) all end up on this
    /// method. The scope is a separate argument rather than a field on
    /// `QueryEventsRequest` **because that type is deserialized from the
    /// request body** — a caller could then widen its own scope by omitting the
    /// field.
    pub fn query_window_scoped(
        &self,
        request: &QueryEventsRequest,
        offset: usize,
        descending: bool,
        scope: &ReadScope,
    ) -> Result<(Vec<Event>, usize)> {
        // Reject a payload filter that cannot be applied, BEFORE doing any
        // work. `apply_filters` parses it per event with `if let Ok(..)`, so an
        // unparseable filter degraded into NO filter: the query answered with
        // every event the caller had asked to exclude, `total_count` agreed,
        // and nothing in the response said the filter had been dropped. A
        // filter must fail closed — loudly — not open.
        if let Some(filter) = &request.payload_filter
            && serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(filter).is_err()
        {
            return Err(AllSourceError::InvalidInput(format!(
                "invalid 'payload_filter': expected a JSON object of field/value \
                 pairs, got '{filter}'"
            )));
        }

        // Lazy-load gate (Step 2): if the request scopes to a tenant,
        // make sure that tenant's persisted data is in memory before
        // running the in-memory index lookup. First call for a cold
        // tenant blocks here for the disk read (single-digit seconds
        // on ~100k events); warm tenants take the DashMap fast path
        // and add no measurable latency.
        //
        // Errors propagate as `Err`; the HTTP layer turns that into
        // a 5xx, which is the explicit "no infinite hangs" contract
        // from the Step 2 acceptance criteria.
        //
        // Unfiltered (cross-tenant) queries — `tenant_id = None` —
        // run against whatever is currently in memory. They cannot
        // pre-load every tenant without defeating the whole point
        // of the lazy-load model. In practice the gateway always
        // injects an auth-derived `tenant_id`; an unfiltered query
        // is admin-only and gets degraded results until a future
        // commit adds an explicit "load all tenants" admin path.
        if let Some(ref tenant_id) = request.tenant_id {
            self.ensure_tenant_loaded(tenant_id)?;
            // LRU touch — the most-recently-queried tenant moves
            // to the back of the eviction queue. Cheap (single
            // DashMap insert), called on every per-tenant query.
            self.tenant_loader.touch(tenant_id);
        }

        // Determine query type for metrics (v0.6 feature)
        let query_type = if request.entity_id.is_some() {
            "entity"
        } else if request.event_type.is_some() {
            "type"
        } else if request.event_type_prefix.is_some() {
            "type_prefix"
        } else {
            "full_scan"
        };

        // Start metrics timer (v0.6 feature)
        #[cfg(feature = "server")]
        let timer = self
            .metrics
            .query_duration_seconds
            .with_label_values(&[query_type])
            .start_timer();

        // Increment query counter (v0.6 feature)
        #[cfg(feature = "server")]
        self.metrics
            .queries_total
            .with_label_values(&[query_type])
            .inc();

        let events = self.events.read();

        // Use index for fast lookups
        let offsets: Vec<usize> = if let Some(entity_id) = &request.entity_id {
            // Use entity index
            self.index
                .get_by_entity(entity_id)
                .map(|entries| self.filter_entries(entries, request))
                .unwrap_or_default()
        } else if let Some(event_type) = &request.event_type {
            // Use type index (exact match)
            self.index
                .get_by_type(event_type)
                .map(|entries| self.filter_entries(entries, request))
                .unwrap_or_default()
        } else if let Some(prefix) = &request.event_type_prefix {
            // Use type index (prefix match)
            let entries = self.index.get_by_type_prefix(prefix);
            self.filter_entries(entries, request)
        } else {
            // Full scan (less efficient but necessary for complex queries)
            (0..events.len()).collect()
        };

        // Apply remaining filters against BORROWED events — cloning happens
        // only for the window that survives the selection below. Each match
        // carries its scan position, which is the final tie-breaker and makes
        // the ordering a *total* order; that is what lets the partial selection
        // below produce exactly what a stable full sort would have.
        let mut matches: Vec<(usize, &Event)> = offsets
            .iter()
            .filter_map(|&event_offset| events.get(event_offset))
            .filter(|event| scope.permits(event.entity_id().as_str()))
            .filter(|event| self.apply_filters(event, request))
            .enumerate()
            .collect();

        // Total is the match count before windowing — callers surface it as
        // `total_count`/`has_more`.
        let total = matches.len();

        // Order by timestamp ascending, with version as a deterministic
        // tie-breaker so events that share a timestamp keep a stable,
        // well-defined order — "the latest event" must be unambiguous
        // (issue #177) — then by scan position, so equal (timestamp, version)
        // pairs keep the order a stable sort gave them. `descending` is the
        // exact reverse of that total order.
        let order = |a: &(usize, &Event), b: &(usize, &Event)| {
            let ascending =
                a.1.timestamp
                    .cmp(&b.1.timestamp)
                    .then_with(|| a.1.version.cmp(&b.1.version))
                    .then_with(|| a.0.cmp(&b.0));
            if descending {
                ascending.reverse()
            } else {
                ascending
            }
        };

        // Bounded selection (issue #251): sort only the requested window.
        select_window(&mut matches, offset, request.limit, order);

        // Apply the offset. `limit` is already accounted for: either the window
        // was truncated by `select_window`, or it covered the whole match set.
        let results: Vec<Event> = matches
            .into_iter()
            .skip(offset)
            .map(|(_, event)| event.clone())
            .collect();

        // Record query results count (v0.6 feature)
        #[cfg(feature = "server")]
        {
            self.metrics
                .query_results_total
                .with_label_values(&[query_type])
                .inc_by(results.len() as u64);
            timer.observe_duration();
        }

        Ok((results, total))
    }

    /// Filter index entries based on query parameters
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    fn filter_entries(&self, entries: Vec<IndexEntry>, request: &QueryEventsRequest) -> Vec<usize> {
        entries
            .into_iter()
            .filter(|entry| {
                // Time filters
                if let Some(as_of) = request.as_of
                    && entry.timestamp > as_of
                {
                    return false;
                }
                if let Some(since) = request.since
                    && entry.timestamp < since
                {
                    return false;
                }
                if let Some(until) = request.until
                    && entry.timestamp > until
                {
                    return false;
                }
                true
            })
            .map(|entry| entry.offset)
            .collect()
    }

    /// Apply filters to an event
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    fn apply_filters(&self, event: &Event, request: &QueryEventsRequest) -> bool {
        // Tenant isolation: if a tenant_id is specified, only return events from that tenant
        if let Some(ref tid) = request.tenant_id
            && event.tenant_id_str() != tid
        {
            return false;
        }

        // Time range. Also pre-applied to index entries in `filter_entries`
        // (cheaper — it skips the event fetch), but `filter_entries` runs ONLY
        // on the indexed branches. A query that no index narrows — scoped by
        // tenant alone, or by `payload_filter`/`exclude_event_type_prefix` —
        // takes the full-scan branch and never reaches it, so without this
        // `since`/`until`/`as_of` were silently dropped and the query answered
        // with the entire history (`total`/`has_more` included). Re-checking
        // here is idempotent for the indexed paths: same predicate, and an
        // index entry's timestamp is its event's.
        if let Some(as_of) = request.as_of
            && event.timestamp > as_of
        {
            return false;
        }
        if let Some(since) = request.since
            && event.timestamp < since
        {
            return false;
        }
        if let Some(until) = request.until
            && event.timestamp > until
        {
            return false;
        }

        // Exclusion: drop events whose type starts with any excluded prefix
        // (comma-separated). Applied here, before sort+limit, so excluded events
        // never consume the result window.
        if let Some(ref excludes) = request.exclude_event_type_prefix {
            let et = event.event_type_str();
            if excludes
                .split(',')
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .any(|p| et.starts_with(p))
            {
                return false;
            }
        }

        // Additional type filter if entity was primary
        if request.entity_id.is_some()
            && let Some(ref event_type) = request.event_type
            && event.event_type_str() != event_type
        {
            return false;
        }

        // Additional prefix filter if entity was primary
        if request.entity_id.is_some()
            && let Some(ref prefix) = request.event_type_prefix
            && !event.event_type_str().starts_with(prefix)
        {
            return false;
        }

        // Payload field filtering
        if let Some(ref filter_str) = request.payload_filter
            && let Ok(filter_obj) =
                serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(filter_str)
        {
            let payload = event.payload();
            for (key, expected_value) in &filter_obj {
                match payload.get(key) {
                    Some(actual_value) if actual_value == expected_value => {}
                    _ => return false,
                }
            }
        }

        true
    }

    /// Reconstruct entity state as of a specific timestamp
    /// v0.2: Now uses snapshots for fast reconstruction
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    pub fn reconstruct_state(
        &self,
        entity_id: &str,
        as_of: Option<DateTime<Utc>>,
    ) -> Result<serde_json::Value> {
        // Try to find a snapshot to use as a base (v0.2 optimization)
        let (merged_state, since_timestamp) = if let Some(as_of_time) = as_of {
            // Get snapshot closest to requested time
            if let Some(snapshot) = self
                .snapshot_manager
                .get_snapshot_as_of(entity_id, as_of_time)
            {
                tracing::debug!(
                    "Using snapshot from {} for entity {} (saved {} events)",
                    snapshot.as_of,
                    entity_id,
                    snapshot.event_count
                );
                (snapshot.state.clone(), Some(snapshot.as_of))
            } else {
                (serde_json::json!({}), None)
            }
        } else {
            // Get latest snapshot for current state
            if let Some(snapshot) = self.snapshot_manager.get_latest_snapshot(entity_id) {
                tracing::debug!(
                    "Using latest snapshot from {} for entity {}",
                    snapshot.as_of,
                    entity_id
                );
                (snapshot.state.clone(), Some(snapshot.as_of))
            } else {
                (serde_json::json!({}), None)
            }
        };

        // Query events after the snapshot (or all if no snapshot)
        let events = self.query(&QueryEventsRequest {
            entity_id: Some(entity_id.to_string()),
            event_type: None,
            tenant_id: None,
            as_of,
            since: since_timestamp,
            until: None,
            limit: None,
            event_type_prefix: None,
            exclude_event_type_prefix: None,
            payload_filter: None,
        })?;

        // If no events and no snapshot, entity not found
        if events.is_empty() && since_timestamp.is_none() {
            return Err(AllSourceError::EntityNotFound(entity_id.to_string()));
        }

        // Merge events on top of snapshot (or from scratch if no snapshot)
        let mut merged_state = merged_state;
        for event in &events {
            if let serde_json::Value::Object(ref mut state_map) = merged_state
                && let serde_json::Value::Object(ref payload_map) = event.payload
            {
                for (key, value) in payload_map {
                    state_map.insert(key.clone(), value.clone());
                }
            }
        }

        // Wrap with metadata
        let state = serde_json::json!({
            "entity_id": entity_id,
            "last_updated": events.last().map(|e| e.timestamp),
            "event_count": events.len(),
            "as_of": as_of,
            "current_state": merged_state,
            "history": events.iter().map(|e| {
                serde_json::json!({
                    "event_id": e.id,
                    "type": e.event_type,
                    "timestamp": e.timestamp,
                    "payload": e.payload
                })
            }).collect::<Vec<_>>()
        });

        Ok(state)
    }

    /// Get snapshot from projection (faster than reconstructing)
    pub fn get_snapshot(&self, entity_id: &str) -> Result<serde_json::Value> {
        let projections = self.projections.read();

        if let Some(snapshot_projection) = projections.get_projection("entity_snapshots")
            && let Some(state) = snapshot_projection.get_state(entity_id)
        {
            return Ok(serde_json::json!({
                "entity_id": entity_id,
                "snapshot": state,
                "from_projection": "entity_snapshots"
            }));
        }

        Err(AllSourceError::EntityNotFound(entity_id.to_string()))
    }

    /// Get statistics about the event store
    pub fn stats(&self) -> StoreStats {
        let events = self.events.read();
        let index_stats = self.index.stats();

        StoreStats {
            total_events: events.len(),
            total_entities: index_stats.total_entities,
            total_event_types: index_stats.total_event_types,
            total_ingested: *self.total_ingested.read(),
        }
    }

    /// Get all unique streams (entity_ids) in the store
    pub fn list_streams(&self) -> Vec<StreamInfo> {
        self.index
            .get_all_entities()
            .into_iter()
            .map(|entity_id| {
                let event_count = self
                    .index
                    .get_by_entity(&entity_id)
                    .map_or(0, |entries| entries.len());
                let last_event_at = self
                    .index
                    .get_by_entity(&entity_id)
                    .and_then(|entries| entries.last().map(|e| e.timestamp));
                StreamInfo {
                    stream_id: entity_id,
                    event_count,
                    last_event_at,
                }
            })
            .collect()
    }

    /// Get all unique event types in the store
    pub fn list_event_types(&self) -> Vec<EventTypeInfo> {
        self.index
            .get_all_types()
            .into_iter()
            .map(|event_type| {
                let event_count = self
                    .index
                    .get_by_type(&event_type)
                    .map_or(0, |entries| entries.len());
                let last_event_at = self
                    .index
                    .get_by_type(&event_type)
                    .and_then(|entries| entries.last().map(|e| e.timestamp));
                EventTypeInfo {
                    event_type,
                    event_count,
                    last_event_at,
                }
            })
            .collect()
    }

    // Tenant-scoped variants. The entity_index / type_index are GLOBAL (no tenant
    // dimension), so `list_streams` / `list_event_types` above span every tenant —
    // wrong + a cross-tenant spill on the per-tenant dashboard. These filter by the
    // event's tenant_id so the dashboard's "your" streams / event types / totals
    // reflect only the caller's tenant.

    /// Distinct entities (+ per-entity event count) for ONE tenant.
    pub fn list_streams_for_tenant(&self, tenant_id: &str) -> Vec<StreamInfo> {
        let _ = self.ensure_tenant_loaded(tenant_id);
        let events = self.events.read();
        let mut by_entity: std::collections::HashMap<&str, (usize, chrono::DateTime<chrono::Utc>)> =
            std::collections::HashMap::new();
        for ev in events.iter() {
            if ev.tenant_id_str() != tenant_id {
                continue;
            }
            let e = by_entity
                .entry(ev.entity_id_str())
                .or_insert((0, ev.timestamp));
            e.0 += 1;
            if ev.timestamp > e.1 {
                e.1 = ev.timestamp;
            }
        }
        by_entity
            .into_iter()
            .map(|(entity_id, (count, last))| StreamInfo {
                stream_id: entity_id.to_string(),
                event_count: count,
                last_event_at: Some(last),
            })
            .collect()
    }

    /// Distinct event types (+ per-type event count) for ONE tenant.
    pub fn list_event_types_for_tenant(&self, tenant_id: &str) -> Vec<EventTypeInfo> {
        let _ = self.ensure_tenant_loaded(tenant_id);
        let events = self.events.read();
        let mut by_type: std::collections::HashMap<&str, (usize, chrono::DateTime<chrono::Utc>)> =
            std::collections::HashMap::new();
        for ev in events.iter() {
            if ev.tenant_id_str() != tenant_id {
                continue;
            }
            let e = by_type
                .entry(ev.event_type_str())
                .or_insert((0, ev.timestamp));
            e.0 += 1;
            if ev.timestamp > e.1 {
                e.1 = ev.timestamp;
            }
        }
        by_type
            .into_iter()
            .map(|(event_type, (count, last))| EventTypeInfo {
                event_type: event_type.to_string(),
                event_count: count,
                last_event_at: Some(last),
            })
            .collect()
    }

    /// Event-store statistics for ONE tenant.
    ///
    /// `stats()` above is global (`events.len()` + the global index), so it must
    /// never be served to a tenant — it would leak whole-fleet totals. This is
    /// the scoped equivalent the gateway exposes as `GET /api/v1/stats` with an
    /// auth-derived `tenant_id` (#230).
    ///
    /// Returns the same four counters as `StoreStats`, plus the per-type census
    /// and time range that callers otherwise reconstruct by paging events.
    pub fn stats_for_tenant(&self, tenant_id: &str) -> TenantStoreStats {
        let _ = self.ensure_tenant_loaded(tenant_id);
        let events = self.events.read();

        let mut entities: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut census: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        let mut total_events = 0usize;
        let mut oldest: Option<chrono::DateTime<chrono::Utc>> = None;
        let mut newest: Option<chrono::DateTime<chrono::Utc>> = None;

        for ev in events.iter() {
            if ev.tenant_id_str() != tenant_id {
                continue;
            }

            total_events += 1;
            entities.insert(ev.entity_id_str());
            *census.entry(ev.event_type_str()).or_insert(0) += 1;

            let ts = ev.timestamp;
            if oldest.is_none_or(|o| ts < o) {
                oldest = Some(ts);
            }
            if newest.is_none_or(|n| ts > n) {
                newest = Some(ts);
            }
        }

        TenantStoreStats {
            total_events,
            total_entities: entities.len(),
            total_event_types: census.len(),
            // Scoped equivalent of `total_ingested`: that counter is a global
            // process-lifetime tally with no tenant dimension, so reporting it
            // here would leak. The tenant's own total is the honest answer.
            total_ingested: total_events as u64,
            event_types: census
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
            oldest_event: oldest,
            newest_event: newest,
        }
    }

    /// Reconstruct entity state for ONE tenant.
    ///
    /// Deliberately does NOT use the snapshot fast path that
    /// `reconstruct_state` takes: `Snapshot` carries no `tenant_id` and the
    /// snapshot manager is keyed by `entity_id` alone, so seeding from a
    /// snapshot could fold another tenant's state into this answer when two
    /// tenants share an entity_id. Folding tenant-filtered events is slower and
    /// fails closed (#230).
    pub fn reconstruct_state_for_tenant(
        &self,
        entity_id: &str,
        as_of: Option<DateTime<Utc>>,
        tenant_id: &str,
    ) -> Result<serde_json::Value> {
        let events = self.query(&QueryEventsRequest {
            entity_id: Some(entity_id.to_string()),
            event_type: None,
            tenant_id: Some(tenant_id.to_string()),
            as_of,
            since: None,
            until: None,
            limit: None,
            event_type_prefix: None,
            exclude_event_type_prefix: None,
            payload_filter: None,
        })?;

        if events.is_empty() {
            return Err(AllSourceError::EntityNotFound(entity_id.to_string()));
        }

        let mut merged_state = serde_json::json!({});
        for event in &events {
            if let serde_json::Value::Object(ref mut state_map) = merged_state
                && let serde_json::Value::Object(ref payload_map) = event.payload
            {
                for (key, value) in payload_map {
                    state_map.insert(key.clone(), value.clone());
                }
            }
        }

        Ok(serde_json::json!({
            "entity_id": entity_id,
            "last_updated": events.last().map(|e| e.timestamp),
            "event_count": events.len(),
            "as_of": as_of,
            "current_state": merged_state,
            "history": events.iter().map(|e| {
                serde_json::json!({
                    "event_id": e.id,
                    "type": e.event_type,
                    "timestamp": e.timestamp,
                    "payload": e.payload
                })
            }).collect::<Vec<_>>()
        }))
    }

    /// Attach a broadcast sender to the WAL for replication.
    ///
    /// Thread-safe: can be called through `Arc<EventStore>` at runtime.
    /// Used during initial setup and during follower → leader promotion.
    /// When set, every WAL append publishes the entry to the broadcast
    /// channel so the WAL shipper can stream it to followers.
    pub fn enable_wal_replication(
        &self,
        tx: tokio::sync::broadcast::Sender<crate::infrastructure::persistence::wal::WALEntry>,
    ) {
        if let Some(ref wal_arc) = self.wal {
            wal_arc.set_replication_tx(tx);
            tracing::info!("WAL replication broadcast enabled");
        } else {
            tracing::warn!("Cannot enable WAL replication: WAL is not configured");
        }
    }

    /// Get a reference to the WAL (if configured).
    /// Used by the replication catch-up protocol to determine oldest available offset.
    pub fn wal(&self) -> Option<&Arc<WriteAheadLog>> {
        self.wal.as_ref()
    }

    /// Get a reference to the Parquet storage (if configured).
    /// Used by the replication catch-up protocol to stream snapshot files to followers.
    pub fn parquet_storage(&self) -> Option<&Arc<RwLock<ParquetStorage>>> {
        self.storage.as_ref()
    }
}

/// Configuration for EventStore
#[derive(Debug, Clone, Default)]
pub struct EventStoreConfig {
    /// Optional directory for persistent Parquet storage (v0.2 feature)
    pub storage_dir: Option<PathBuf>,

    /// Snapshot configuration (v0.2 feature)
    pub snapshot_config: SnapshotConfig,

    /// Optional directory for WAL (Write-Ahead Log) (v0.2 feature)
    pub wal_dir: Option<PathBuf>,

    /// WAL configuration (v0.2 feature)
    pub wal_config: WALConfig,

    /// Compaction configuration (v0.2 feature)
    pub compaction_config: CompactionConfig,

    /// Schema registry configuration (v0.5 feature)
    pub schema_registry_config: SchemaRegistryConfig,

    /// Optional directory for system metadata storage (dogfood feature).
    /// When set, operational metadata (tenants, config, audit) is stored
    /// using AllSource's own event store rather than an external database.
    /// Defaults to `{storage_dir}/__system/` when storage_dir is set.
    pub system_data_dir: Option<PathBuf>,

    /// Name of the default tenant to auto-create on first boot.
    pub bootstrap_tenant: Option<String>,

    /// In-memory cache budget in bytes (Step 3). When the resident
    /// total exceeds this after a load, the LRU tenant is evicted
    /// until the cache fits. `None` (the default in tests) disables
    /// the budget — every loaded tenant stays resident. Production
    /// reads this from the `ALLSOURCE_CACHE_BYTES` env var; see
    /// `from_env`.
    pub cache_byte_budget: Option<u64>,

    /// Cadence of the runtime checkpoint loop, in seconds (Step 6).
    /// Each tick flushes pending Parquet batches and, on success,
    /// truncates the WAL up through the checkpoint. This bounds
    /// dirty-restart replay time to one interval of writes
    /// regardless of total dataset size.
    ///
    /// `None` disables the loop — the WAL still grows but is only
    /// truncated at boot, which is the pre-Step-6 behavior. Tests
    /// default to `None`; production reads
    /// `ALLSOURCE_CHECKPOINT_INTERVAL_SECONDS` (default 60s) via
    /// `from_env_vars`.
    pub checkpoint_interval_secs: Option<u64>,

    /// Open the store read-only (replica mode). See `EventStore::read_only`.
    /// Defaults to `false` (read-write owner). Set by Prime when it fails to
    /// acquire the exclusive data-dir lock because another process owns it.
    pub read_only: bool,
}

impl EventStoreConfig {
    /// Create config with persistent storage enabled
    pub fn with_persistence(storage_dir: impl Into<PathBuf>) -> Self {
        Self {
            storage_dir: Some(storage_dir.into()),
            ..Self::default()
        }
    }

    /// Create config with custom snapshot settings
    pub fn with_snapshots(snapshot_config: SnapshotConfig) -> Self {
        Self {
            snapshot_config,
            ..Self::default()
        }
    }

    /// Create config with WAL enabled
    pub fn with_wal(wal_dir: impl Into<PathBuf>, wal_config: WALConfig) -> Self {
        Self {
            wal_dir: Some(wal_dir.into()),
            wal_config,
            ..Self::default()
        }
    }

    /// Create config with both persistence and snapshots
    pub fn with_all(storage_dir: impl Into<PathBuf>, snapshot_config: SnapshotConfig) -> Self {
        Self {
            storage_dir: Some(storage_dir.into()),
            snapshot_config,
            ..Self::default()
        }
    }

    /// Create production config with all features enabled
    pub fn production(
        storage_dir: impl Into<PathBuf>,
        wal_dir: impl Into<PathBuf>,
        snapshot_config: SnapshotConfig,
        wal_config: WALConfig,
        compaction_config: CompactionConfig,
    ) -> Self {
        let storage_dir = storage_dir.into();
        let system_data_dir = storage_dir.join("__system");
        Self {
            storage_dir: Some(storage_dir),
            snapshot_config,
            wal_dir: Some(wal_dir.into()),
            wal_config,
            compaction_config,
            system_data_dir: Some(system_data_dir),
            ..Self::default()
        }
    }

    /// Resolve the effective system data directory.
    ///
    /// If explicitly set, returns that. Otherwise, derives from storage_dir.
    /// Returns None if neither is configured (in-memory mode).
    pub fn effective_system_data_dir(&self) -> Option<PathBuf> {
        self.system_data_dir
            .clone()
            .or_else(|| self.storage_dir.as_ref().map(|d| d.join("__system")))
    }

    /// Build config from environment variables.
    ///
    /// Reads `ALLSOURCE_DATA_DIR`, `ALLSOURCE_STORAGE_DIR`, `ALLSOURCE_WAL_DIR`,
    /// and `ALLSOURCE_WAL_ENABLED` to determine persistence mode.
    ///
    /// Returns `(config, description)` where description is a human-readable
    /// summary of the persistence mode for logging.
    pub fn from_env() -> (Self, &'static str) {
        Self::from_env_vars(
            std::env::var("ALLSOURCE_DATA_DIR")
                .ok()
                .filter(|s| !s.is_empty()),
            std::env::var("ALLSOURCE_STORAGE_DIR")
                .ok()
                .filter(|s| !s.is_empty()),
            std::env::var("ALLSOURCE_WAL_DIR")
                .ok()
                .filter(|s| !s.is_empty()),
            std::env::var("ALLSOURCE_WAL_ENABLED").ok(),
            std::env::var("ALLSOURCE_CACHE_BYTES").ok(),
            std::env::var("ALLSOURCE_SNAPSHOT_INTERVAL_SECONDS").ok(),
            std::env::var("ALLSOURCE_RETENTION_SYSTEM_DAYS").ok(),
            std::env::var("ALLSOURCE_CHECKPOINT_INTERVAL_SECONDS").ok(),
        )
    }

    /// Build config from explicit env-var values (testable without mutating process env).
    pub fn from_env_vars(
        data_dir: Option<String>,
        explicit_storage_dir: Option<String>,
        explicit_wal_dir: Option<String>,
        wal_enabled_var: Option<String>,
        cache_bytes_var: Option<String>,
        snapshot_interval_var: Option<String>,
        retention_system_days_var: Option<String>,
        checkpoint_interval_var: Option<String>,
    ) -> (Self, &'static str) {
        let data_dir = data_dir.filter(|s| !s.is_empty());
        let storage_dir = explicit_storage_dir
            .filter(|s| !s.is_empty())
            .or_else(|| data_dir.as_ref().map(|d| format!("{d}/storage")));
        let wal_dir = explicit_wal_dir
            .filter(|s| !s.is_empty())
            .or_else(|| data_dir.as_ref().map(|d| format!("{d}/wal")));
        let wal_enabled = wal_enabled_var.is_none_or(|v| v == "true");
        // ALLSOURCE_CACHE_BYTES: parse decimal bytes. Unparseable
        // input is logged and ignored rather than failing boot —
        // the unbounded fallback is safe (worst case is the
        // original pre-Step-3 behavior).
        let cache_byte_budget =
            cache_bytes_var
                .filter(|s| !s.is_empty())
                .and_then(|s| match s.parse::<u64>() {
                    Ok(v) => Some(v),
                    Err(e) => {
                        tracing::warn!(
                            "ALLSOURCE_CACHE_BYTES={s:?} could not be parsed as u64: {e}; \
                         cache budget disabled"
                        );
                        None
                    }
                });
        let compaction_config =
            CompactionConfig::from_env_vars(snapshot_interval_var, retention_system_days_var);

        // ALLSOURCE_CHECKPOINT_INTERVAL_SECONDS: parse decimal seconds. The
        // default (60s) only applies when WAL is enabled — there's no
        // checkpoint loop to run otherwise. Unparseable input is logged
        // and falls back to the default rather than failing boot.
        let checkpoint_interval_secs = if wal_enabled {
            checkpoint_interval_var
                .filter(|s| !s.is_empty())
                .map(|s| match s.parse::<u64>() {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!(
                            "ALLSOURCE_CHECKPOINT_INTERVAL_SECONDS={s:?} could not be parsed as \
                             u64: {e}; falling back to default 60s"
                        );
                        60
                    }
                })
                .or(Some(60))
        } else {
            None
        };

        let mut config = match (&storage_dir, &wal_dir) {
            (Some(sd), Some(wd)) if wal_enabled => Self::production(
                sd,
                wd,
                SnapshotConfig::default(),
                WALConfig::default(),
                compaction_config,
            ),
            (Some(sd), _) => Self::with_persistence(sd),
            (_, Some(wd)) if wal_enabled => Self::with_wal(wd, WALConfig::default()),
            _ => Self::default(),
        };
        config.cache_byte_budget = cache_byte_budget;
        config.checkpoint_interval_secs = checkpoint_interval_secs;

        let mode = match (&storage_dir, &wal_dir) {
            (Some(_), Some(_)) if wal_enabled => "wal+parquet",
            (Some(_), _) => "parquet-only",
            (_, Some(_)) if wal_enabled => "wal-only",
            _ => "in-memory",
        };
        (config, mode)
    }
}

#[derive(Debug, serde::Serialize)]
pub struct StoreStats {
    pub total_events: usize,
    pub total_entities: usize,
    pub total_event_types: usize,
    pub total_ingested: u64,
}

/// Tenant-scoped event-store statistics (#230).
///
/// Mirrors `StoreStats`' counters so existing consumers keep working, and adds
/// the per-type census and time range that clients previously derived by paging
/// the whole event stream.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TenantStoreStats {
    pub total_events: usize,
    pub total_entities: usize,
    pub total_event_types: usize,
    pub total_ingested: u64,
    /// event_type -> count, for this tenant only.
    pub event_types: std::collections::HashMap<String, usize>,
    pub oldest_event: Option<chrono::DateTime<chrono::Utc>>,
    pub newest_event: Option<chrono::DateTime<chrono::Utc>>,
}

/// Information about a stream (entity_id)
#[derive(Debug, Clone, serde::Serialize)]
pub struct StreamInfo {
    /// The stream identifier (entity_id)
    pub stream_id: String,
    /// Total number of events in this stream
    pub event_count: usize,
    /// Timestamp of the last event in this stream
    pub last_event_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Information about an event type
#[derive(Debug, Clone, serde::Serialize)]
pub struct EventTypeInfo {
    /// The event type name
    pub event_type: String,
    /// Total number of events of this type
    pub event_count: usize,
    /// Timestamp of the last event of this type
    pub last_event_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Default for EventStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::Event;
    use tempfile::TempDir;

    /// Recursively walk `dir` looking for `*.parquet` files.
    /// Tests that pre-date Step 1's tenant-partitioned layout used a
    /// flat `read_dir` here; after the move to <root>/<tenant>/<yyyy-mm>/
    /// they need to walk subdirectories.
    fn find_parquet_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().and_then(|s| s.to_str()) == Some("parquet") {
                    out.push(p);
                }
            }
        }
        out
    }

    fn create_test_event(entity_id: &str, event_type: &str) -> Event {
        Event::from_strings(
            event_type.to_string(),
            entity_id.to_string(),
            "default".to_string(),
            serde_json::json!({"name": "Test", "value": 42}),
            None,
        )
        .unwrap()
    }

    fn create_test_event_with_payload(
        entity_id: &str,
        event_type: &str,
        payload: serde_json::Value,
    ) -> Event {
        Event::from_strings(
            event_type.to_string(),
            entity_id.to_string(),
            "default".to_string(),
            payload,
            None,
        )
        .unwrap()
    }

    #[test]
    fn test_event_store_new() {
        let store = EventStore::new();
        assert_eq!(store.stats().total_events, 0);
        assert_eq!(store.stats().total_entities, 0);
    }

    // -----------------------------------------------------------------
    // Step 2: ensure_tenant_loaded smoke tests. The full
    // cold-boot/lazy-hydrate paths land in commit #2 (skip boot
    // load) and commit #4 (integration test).
    // -----------------------------------------------------------------

    #[test]
    fn test_ensure_tenant_loaded_no_storage_is_a_noop() {
        // An in-memory-only store (no ParquetStorage configured) has
        // nothing to hydrate. The method must succeed and mark the
        // tenant loaded so subsequent calls hit the fast path.
        let store = EventStore::new();
        assert!(!store.is_tenant_loaded("alice"));
        store.ensure_tenant_loaded("alice").unwrap();
        assert!(store.is_tenant_loaded("alice"));
        // Other tenants stay cold — the call is per-tenant.
        assert!(!store.is_tenant_loaded("bob"));
    }

    #[test]
    fn test_ensure_tenant_loaded_warm_path_is_idempotent() {
        let store = EventStore::new();
        store.ensure_tenant_loaded("alice").unwrap();
        // Second call hits the DashMap fast path and returns Ok.
        store.ensure_tenant_loaded("alice").unwrap();
    }

    #[test]
    fn test_ensure_tenant_loaded_rejects_unsafe_tenant_id() {
        // With persistence configured, the call has to walk a
        // tenant subtree, so the path-safety whitelist applies.
        // The error must propagate; the tenant must NOT be marked
        // loaded (otherwise an attacker probing path-traversal
        // strings could spam the loaded-set with junk).
        let temp_dir = TempDir::new().unwrap();
        let store = EventStore::with_config(EventStoreConfig::with_persistence(temp_dir.path()));
        for unsafe_tid in ["..", "a/b", "a\\b", ""] {
            let result = store.ensure_tenant_loaded(unsafe_tid);
            assert!(
                result.is_err(),
                "tenant_id {unsafe_tid:?} should have been rejected"
            );
            assert!(
                !store.is_tenant_loaded(unsafe_tid),
                "rejected tenant {unsafe_tid:?} must not be marked loaded"
            );
        }
    }

    #[test]
    fn test_ensure_tenant_loaded_no_subtree_marks_loaded_with_zero_events() {
        // A tenant that has no on-disk data (fresh tenant, never
        // persisted) must still succeed — load_events_for_tenant
        // returns empty, ensure_tenant_loaded marks it loaded so we
        // don't re-walk the empty subtree on every query.
        let temp_dir = TempDir::new().unwrap();
        let store = EventStore::with_config(EventStoreConfig::with_persistence(temp_dir.path()));
        assert!(!store.is_tenant_loaded("never-existed"));
        store.ensure_tenant_loaded("never-existed").unwrap();
        assert!(store.is_tenant_loaded("never-existed"));
    }

    #[test]
    fn test_evict_tenant_drops_events_and_resets_bytes() {
        // After eviction, the tenant's events are gone from memory,
        // its byte counter is reset, and is_tenant_loaded returns
        // false. Other tenants are untouched.
        let temp_dir = TempDir::new().unwrap();
        let storage_dir = temp_dir.path().to_path_buf();

        {
            let store = EventStore::with_config(EventStoreConfig::with_persistence(&storage_dir));
            for i in 0..3 {
                store
                    .ingest(
                        &Event::from_strings(
                            "test.event".to_string(),
                            format!("a-{i}"),
                            "alice".to_string(),
                            serde_json::json!({"i": i}),
                            None,
                        )
                        .unwrap(),
                    )
                    .unwrap();
            }
            for i in 0..2 {
                store
                    .ingest(
                        &Event::from_strings(
                            "test.event".to_string(),
                            format!("b-{i}"),
                            "bob".to_string(),
                            serde_json::json!({"i": i}),
                            None,
                        )
                        .unwrap(),
                    )
                    .unwrap();
            }
            store.flush_storage().unwrap();
        }

        let store = EventStore::with_config(EventStoreConfig::with_persistence(&storage_dir));
        store.ensure_tenant_loaded("alice").unwrap();
        store.ensure_tenant_loaded("bob").unwrap();
        assert_eq!(store.stats().total_events, 5);
        let alice_bytes = store.tenant_resident_bytes("alice");
        let bob_bytes = store.tenant_resident_bytes("bob");
        assert!(alice_bytes > 0 && bob_bytes > 0);

        store.evict_tenant("alice");

        assert!(!store.is_tenant_loaded("alice"));
        assert!(store.is_tenant_loaded("bob"));
        assert_eq!(store.tenant_resident_bytes("alice"), 0);
        assert_eq!(store.tenant_resident_bytes("bob"), bob_bytes);
        assert_eq!(store.stats().total_events, 2, "only bob's 2 events remain");
    }

    #[test]
    fn test_evict_tenant_then_query_re_loads_from_disk() {
        // The transparent re-load behavior the bead's AC #5 calls
        // out: evict, then query the same tenant — its data comes
        // back via ensure_tenant_loaded, sourced from Parquet.
        let temp_dir = TempDir::new().unwrap();
        let storage_dir = temp_dir.path().to_path_buf();

        {
            let store = EventStore::with_config(EventStoreConfig::with_persistence(&storage_dir));
            for i in 0..4 {
                store
                    .ingest(
                        &Event::from_strings(
                            "test.event".to_string(),
                            format!("a-{i}"),
                            "alice".to_string(),
                            serde_json::json!({"i": i}),
                            None,
                        )
                        .unwrap(),
                    )
                    .unwrap();
            }
            store.flush_storage().unwrap();
        }

        let store = EventStore::with_config(EventStoreConfig::with_persistence(&storage_dir));
        store.ensure_tenant_loaded("alice").unwrap();
        store.evict_tenant("alice");
        assert_eq!(store.stats().total_events, 0);

        // Query — re-load happens transparently.
        let results = store
            .query(&QueryEventsRequest {
                entity_id: None,
                event_type: None,
                tenant_id: Some("alice".to_string()),
                as_of: None,
                since: None,
                until: None,
                limit: None,
                event_type_prefix: None,
                exclude_event_type_prefix: None,
                payload_filter: None,
            })
            .unwrap();
        assert_eq!(results.len(), 4);
        assert!(store.is_tenant_loaded("alice"));
    }

    #[test]
    fn test_evict_tenant_rebuilds_index_with_new_offsets() {
        // After eviction, the events Vec is compacted. The index
        // must be rebuilt against the new offsets — otherwise
        // queries return stale or wrong events. This test checks
        // index correctness end-to-end via a query for the
        // surviving tenant after the evicted tenant's events are
        // gone.
        let temp_dir = TempDir::new().unwrap();
        let store = EventStore::with_config(EventStoreConfig::with_persistence(temp_dir.path()));

        // Interleave: alice, bob, alice, bob, alice. After
        // evicting alice, the events Vec compacts to [bob, bob]
        // and the index must reflect the new layout.
        for i in 0..3 {
            store
                .ingest(
                    &Event::from_strings(
                        "test.event".to_string(),
                        format!("a-{i}"),
                        "alice".to_string(),
                        serde_json::json!({"i": i}),
                        None,
                    )
                    .unwrap(),
                )
                .unwrap();
            if i < 2 {
                store
                    .ingest(
                        &Event::from_strings(
                            "test.event".to_string(),
                            format!("b-{i}"),
                            "bob".to_string(),
                            serde_json::json!({"i": i}),
                            None,
                        )
                        .unwrap(),
                    )
                    .unwrap();
            }
        }
        // Mark both as loaded for accurate eviction bookkeeping.
        store.tenant_loader.mark_loaded("alice");
        store.tenant_loader.mark_loaded("bob");

        store.evict_tenant("alice");

        let bob_results = store
            .query(&QueryEventsRequest {
                entity_id: None,
                event_type: None,
                tenant_id: Some("bob".to_string()),
                as_of: None,
                since: None,
                until: None,
                limit: None,
                event_type_prefix: None,
                exclude_event_type_prefix: None,
                payload_filter: None,
            })
            .unwrap();
        assert_eq!(bob_results.len(), 2);
        for e in &bob_results {
            assert_eq!(e.tenant_id_str(), "bob");
        }
    }

    #[test]
    fn test_budget_eviction_keeps_resident_set_bounded() {
        // Configure a tiny budget. Load three tenants in sequence;
        // the third load must evict the LRU tenant, keeping the
        // resident set under (or near) the budget.
        let temp_dir = TempDir::new().unwrap();
        let storage_dir = temp_dir.path().to_path_buf();

        // Persist 5 events per tenant with ~1 KiB payloads. Each
        // tenant ends up at ~5 KiB + overhead.
        let big_payload = serde_json::json!({"data": "x".repeat(1000)});
        {
            let store = EventStore::with_config(EventStoreConfig::with_persistence(&storage_dir));
            for tenant in ["alice", "bob", "carol"] {
                for i in 0..5 {
                    store
                        .ingest(
                            &Event::from_strings(
                                "test.event".to_string(),
                                format!("{tenant}-{i}"),
                                tenant.to_string(),
                                big_payload.clone(),
                                None,
                            )
                            .unwrap(),
                        )
                        .unwrap();
                }
            }
            store.flush_storage().unwrap();
        }

        // Budget = 12 KiB. Two tenants (~6 KiB each = ~12 KiB) is
        // tight; loading a third must evict.
        let mut config = EventStoreConfig::with_persistence(&storage_dir);
        config.cache_byte_budget = Some(12_000);
        let store = EventStore::with_config(config);

        // Load alice — under budget, no eviction.
        store.ensure_tenant_loaded("alice").unwrap();
        assert!(store.is_tenant_loaded("alice"));

        // Touch alice and immediately load bob. Bob is the
        // freshly-loaded one, so bob is excluded from eviction.
        // Alice is the next-oldest. After the load, total may
        // exceed budget — if so, evict alice.
        store.tenant_loader.touch("alice");
        std::thread::sleep(std::time::Duration::from_millis(10));
        store.ensure_tenant_loaded("bob").unwrap();
        assert!(store.is_tenant_loaded("bob"));

        // Touch bob, load carol. Carol is freshly-loaded; the LRU
        // candidate is the older of {alice, bob} — alice (since
        // bob was just touched).
        store.tenant_loader.touch("bob");
        std::thread::sleep(std::time::Duration::from_millis(10));
        store.ensure_tenant_loaded("carol").unwrap();
        assert!(store.is_tenant_loaded("carol"));

        // After all loads, the cache must respect the budget OR
        // (if a single tenant alone exceeds it) we should at most
        // hold the just-loaded tenant. The test budget is small
        // enough that we expect at least one eviction.
        let resident = store.cache_resident_bytes();
        let budget = 12_000u64;

        // Either we're within the budget, or only the freshly-loaded
        // tenant is left (the "single oversized tenant" fallback).
        if resident > budget {
            let loaded_count = ["alice", "bob", "carol"]
                .iter()
                .filter(|t| store.is_tenant_loaded(t))
                .count();
            assert_eq!(
                loaded_count, 1,
                "over budget but more than one tenant loaded — eviction policy didn't fire"
            );
        }

        // Carol must still be loaded — it's the most recent and
        // never picked as a victim.
        assert!(store.is_tenant_loaded("carol"));
    }

    #[test]
    fn test_query_after_eviction_re_loads_transparently() {
        // The end-to-end shape of AC #5: query → evict → query
        // again returns the right data.
        let temp_dir = TempDir::new().unwrap();
        let storage_dir = temp_dir.path().to_path_buf();

        let big_payload = serde_json::json!({"data": "x".repeat(2000)});
        {
            let store = EventStore::with_config(EventStoreConfig::with_persistence(&storage_dir));
            for tenant in ["alice", "bob"] {
                for i in 0..3 {
                    store
                        .ingest(
                            &Event::from_strings(
                                "test.event".to_string(),
                                format!("{tenant}-{i}"),
                                tenant.to_string(),
                                big_payload.clone(),
                                None,
                            )
                            .unwrap(),
                        )
                        .unwrap();
                }
            }
            store.flush_storage().unwrap();
        }

        // Budget = 5 KiB — one tenant fits, two don't.
        let mut config = EventStoreConfig::with_persistence(&storage_dir);
        config.cache_byte_budget = Some(5_000);
        let store = EventStore::with_config(config);

        // Query alice — sized at ~6 KiB, so over budget but no
        // peer to evict; alice stays as the single-oversized-tenant
        // case.
        let alice_first = store
            .query(&QueryEventsRequest {
                entity_id: None,
                event_type: None,
                tenant_id: Some("alice".to_string()),
                as_of: None,
                since: None,
                until: None,
                limit: None,
                event_type_prefix: None,
                exclude_event_type_prefix: None,
                payload_filter: None,
            })
            .unwrap();
        assert_eq!(alice_first.len(), 3);

        // Sleep to make alice older than bob in the LRU ordering.
        std::thread::sleep(std::time::Duration::from_millis(15));
        // Query bob — alice will get evicted.
        let _bob = store
            .query(&QueryEventsRequest {
                entity_id: None,
                event_type: None,
                tenant_id: Some("bob".to_string()),
                as_of: None,
                since: None,
                until: None,
                limit: None,
                event_type_prefix: None,
                exclude_event_type_prefix: None,
                payload_filter: None,
            })
            .unwrap();
        assert!(
            !store.is_tenant_loaded("alice"),
            "alice should have been evicted"
        );

        // Re-query alice — must transparently re-load.
        let alice_second = store
            .query(&QueryEventsRequest {
                entity_id: None,
                event_type: None,
                tenant_id: Some("alice".to_string()),
                as_of: None,
                since: None,
                until: None,
                limit: None,
                event_type_prefix: None,
                exclude_event_type_prefix: None,
                payload_filter: None,
            })
            .unwrap();
        assert_eq!(
            alice_second.len(),
            3,
            "alice's events come back via re-load"
        );
        assert!(store.is_tenant_loaded("alice"));
    }

    #[test]
    #[cfg(feature = "server")]
    fn test_cache_metrics_track_evictions_and_bytes() {
        // Smoke test for the Step 3 #4 Prometheus metrics —
        // confirms the counter increments on eviction and the
        // gauge tracks the resident bytes.
        let temp_dir = TempDir::new().unwrap();
        let storage_dir = temp_dir.path().to_path_buf();

        let big_payload = serde_json::json!({"data": "x".repeat(2000)});
        {
            let store = EventStore::with_config(EventStoreConfig::with_persistence(&storage_dir));
            for tenant in ["alice", "bob"] {
                for i in 0..3 {
                    store
                        .ingest(
                            &Event::from_strings(
                                "test.event".to_string(),
                                format!("{tenant}-{i}"),
                                tenant.to_string(),
                                big_payload.clone(),
                                None,
                            )
                            .unwrap(),
                        )
                        .unwrap();
                }
            }
            store.flush_storage().unwrap();
        }

        let mut config = EventStoreConfig::with_persistence(&storage_dir);
        config.cache_byte_budget = Some(5_000); // forces eviction
        let store = EventStore::with_config(config);

        assert_eq!(store.metrics.cache_evictions_total.get(), 0);
        assert_eq!(store.metrics.cache_bytes.get(), 0);

        store.ensure_tenant_loaded("alice").unwrap();
        // After loading alice, gauge reflects her bytes.
        let after_alice = store.metrics.cache_bytes.get();
        assert!(after_alice > 0, "gauge should reflect alice's bytes");
        // Single oversized tenant — no eviction yet.
        assert_eq!(store.metrics.cache_evictions_total.get(), 0);

        std::thread::sleep(std::time::Duration::from_millis(10));
        store.ensure_tenant_loaded("bob").unwrap();

        // Bob's load pushed total over budget; alice (older) was
        // evicted. Counter increments.
        assert_eq!(
            store.metrics.cache_evictions_total.get(),
            1,
            "exactly one tenant evicted after bob's load"
        );
        // Gauge now reflects only bob's bytes.
        let after_bob = store.metrics.cache_bytes.get();
        assert!(after_bob > 0);
        assert!(after_bob <= after_alice, "gauge dropped after eviction");
    }

    #[test]
    #[cfg(feature = "server")]
    fn test_storage_size_gauge_populated_from_on_disk_bytes() {
        // The allsource_storage_size_bytes / _parquet_files_total / _wal_segments_total
        // gauges used to be registered but never set (constant 0), so the dashboard's
        // "storage" card showed "—". refresh_storage_metrics must populate them from
        // the real on-disk footprint (Parquet bytes + WAL segment bytes).
        let temp_dir = TempDir::new().unwrap();
        // Configure BOTH Parquet persistence and a WAL so the gauge exercises the
        // full `parquet_bytes + wal_bytes` summation (production runs with both).
        let config = EventStoreConfig {
            storage_dir: Some(temp_dir.path().join("parquet")),
            wal_dir: Some(temp_dir.path().join("wal")),
            ..EventStoreConfig::default()
        };
        let store = EventStore::with_config(config);

        // Gauge starts at 0 before anything is written/refreshed.
        assert_eq!(store.metrics.storage_size_bytes.get(), 0);

        let payload = serde_json::json!({ "data": "x".repeat(2000) });
        for i in 0..10 {
            store
                .ingest(
                    &Event::from_strings(
                        "test.event".to_string(),
                        format!("entity-{i}"),
                        "tenant-a".to_string(),
                        payload.clone(),
                        None,
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        store.flush_storage().unwrap();

        // Populate the gauges from disk (what the boot hook + checkpoint loop call).
        store.refresh_storage_metrics_now();

        let size = store.metrics.storage_size_bytes.get();
        assert!(
            size > 0,
            "storage_size_bytes must reflect real on-disk bytes, got {size}"
        );
        assert!(
            store.metrics.parquet_files_total.get() >= 1,
            "at least one Parquet file should exist after a flush"
        );

        // WAL segment count is surfaced too: with a WAL configured and writes done,
        // there is at least one segment on disk.
        assert!(
            store.metrics.wal_segments_total.get() >= 1,
            "at least one WAL segment should exist after writes"
        );

        // Sanity: the reported size is the Parquet bytes plus the WAL bytes — i.e.
        // it's the real on-disk footprint, not a stale/placeholder value.
        let parquet_stats = store.storage.as_ref().unwrap().read().stats().unwrap();
        let (wal_bytes, _) = store.wal.as_ref().unwrap().on_disk_stats().unwrap();
        assert_eq!(
            size as u64,
            parquet_stats.total_size_bytes + wal_bytes,
            "gauge should equal Parquet bytes ({}) + WAL bytes ({wal_bytes})",
            parquet_stats.total_size_bytes
        );
    }

    #[test]
    fn test_stress_resident_set_stays_near_budget_under_rolling_queries() {
        // Scaled-down version of the bead's stress test: the
        // bead's 10 × 50 MB / 100 MB ratio (10× tenants vs
        // budget-headroom) preserved at 500 KB / 1 MB to stay
        // unit-test-fast. The same correctness property: after
        // many rolling queries across more tenants than fit, the
        // resident set must stay at-or-near the budget.
        let temp_dir = TempDir::new().unwrap();
        let storage_dir = temp_dir.path().to_path_buf();

        const TENANT_COUNT: usize = 10;
        const EVENTS_PER_TENANT: usize = 50;
        // Per-event payload ~10 KiB → tenant ~ 500 KiB.
        let big_payload = serde_json::json!({"data": "x".repeat(10_000)});

        // Persist all tenants. Each ends up at ~500 KiB on disk
        // (and roughly the same in memory once loaded).
        {
            let store = EventStore::with_config(EventStoreConfig::with_persistence(&storage_dir));
            for t in 0..TENANT_COUNT {
                let tenant = format!("tenant-{t}");
                for i in 0..EVENTS_PER_TENANT {
                    store
                        .ingest(
                            &Event::from_strings(
                                "test.event".to_string(),
                                format!("{tenant}-{i}"),
                                tenant.clone(),
                                big_payload.clone(),
                                None,
                            )
                            .unwrap(),
                        )
                        .unwrap();
                }
            }
            store.flush_storage().unwrap();
        }

        // Budget = 1 MiB → fits ~2 tenants. We're going to query
        // all 10, so the LRU policy must hold the resident set
        // near 1 MiB across the rolling sequence.
        const BUDGET: u64 = 1_048_576;
        let mut config = EventStoreConfig::with_persistence(&storage_dir);
        config.cache_byte_budget = Some(BUDGET);
        let store = EventStore::with_config(config);

        // Sweep through tenants in order. Each query loads its
        // tenant; if budget is exceeded after the load, an LRU
        // eviction fires.
        let mut peak_resident: u64 = 0;
        for t in 0..TENANT_COUNT {
            let tenant = format!("tenant-{t}");
            let results = store
                .query(&QueryEventsRequest {
                    entity_id: None,
                    event_type: None,
                    tenant_id: Some(tenant.clone()),
                    as_of: None,
                    since: None,
                    until: None,
                    limit: None,
                    event_type_prefix: None,
                    exclude_event_type_prefix: None,
                    payload_filter: None,
                })
                .unwrap();
            assert_eq!(
                results.len(),
                EVENTS_PER_TENANT,
                "every per-tenant query must return all of that tenant's events"
            );
            // Track peak resident bytes seen during the sweep.
            let resident = store.cache_resident_bytes();
            if resident > peak_resident {
                peak_resident = resident;
            }
        }

        let final_resident = store.cache_resident_bytes();

        // Tolerance: a tenant's bytes get added before eviction
        // fires, so peak transiently exceeds the budget by at
        // most one tenant's worth (~500 KiB). The final state
        // after the sweep should be well-bounded.
        let tolerance = BUDGET; // generous: 2× budget upper bound
        assert!(
            peak_resident <= BUDGET + tolerance,
            "peak resident {peak_resident} exceeds budget {BUDGET} by more than {tolerance} \
             — eviction policy not keeping up with the working-set churn"
        );
        assert!(
            final_resident <= BUDGET + tolerance,
            "final resident {final_resident} exceeds budget {BUDGET} by more than {tolerance}"
        );

        // The most-recently-queried tenant must still be loaded
        // (it was just touched).
        let last_tenant = format!("tenant-{}", TENANT_COUNT - 1);
        assert!(
            store.is_tenant_loaded(&last_tenant),
            "the most-recent tenant must remain loaded after the sweep"
        );

        // At least some tenants must have been evicted — otherwise
        // the budget didn't fire.
        let still_loaded = (0..TENANT_COUNT)
            .filter(|t| store.is_tenant_loaded(&format!("tenant-{t}")))
            .count();
        assert!(
            still_loaded < TENANT_COUNT,
            "no tenants evicted ({still_loaded}/{TENANT_COUNT} still loaded) — \
             budget enforcement didn't engage"
        );
    }

    #[test]
    fn test_evict_tenant_when_not_loaded_is_a_noop() {
        // Eviction of a never-loaded tenant must not panic and
        // must not affect other tenants.
        let store = EventStore::new();
        store.evict_tenant("nobody"); // should not panic
        assert!(!store.is_tenant_loaded("nobody"));
    }

    #[test]
    fn test_lazy_load_accounts_bytes_per_tenant() {
        // Step 3 #1: per-tenant byte tracking. Loading a tenant
        // should accumulate bytes proportional to its event
        // payload sizes; another tenant's counter must stay 0.
        let temp_dir = TempDir::new().unwrap();
        let storage_dir = temp_dir.path().to_path_buf();

        // Persist 5 events for alice with measurable-size payloads.
        {
            let store = EventStore::with_config(EventStoreConfig::with_persistence(&storage_dir));
            for i in 0..5 {
                store
                    .ingest(
                        &Event::from_strings(
                            "test.event".to_string(),
                            format!("a-{i}"),
                            "alice".to_string(),
                            serde_json::json!({"data": "x".repeat(1000)}),
                            None,
                        )
                        .unwrap(),
                    )
                    .unwrap();
            }
            store.flush_storage().unwrap();
        }

        let store = EventStore::with_config(EventStoreConfig::with_persistence(&storage_dir));
        // Cold: zero bytes accounted.
        assert_eq!(store.tenant_resident_bytes("alice"), 0);
        assert_eq!(store.cache_resident_bytes(), 0);

        store.ensure_tenant_loaded("alice").unwrap();

        // After load: alice's counter is non-trivial (5 events
        // each carrying ~1000 bytes of payload + overhead).
        let alice_bytes = store.tenant_resident_bytes("alice");
        assert!(
            alice_bytes >= 5 * 1000,
            "alice should have at least 5 KiB resident; got {alice_bytes}"
        );
        // Bob never loaded → 0.
        assert_eq!(store.tenant_resident_bytes("bob"), 0);
        // Total equals alice's portion (only loaded tenant).
        assert_eq!(store.cache_resident_bytes(), alice_bytes);
    }

    #[test]
    fn test_query_lazy_loads_tenant_on_first_call() {
        // The end-to-end shape of Step 2: persist events for a
        // tenant in session 1, restart, and confirm session 2 boots
        // empty but a query for that tenant pulls them in.
        let temp_dir = TempDir::new().unwrap();
        let storage_dir = temp_dir.path().to_path_buf();

        // Session 1: ingest 3 events for tenant "alice", flush, drop.
        {
            let store = EventStore::with_config(EventStoreConfig::with_persistence(&storage_dir));
            for i in 0..3 {
                let event = Event::from_strings(
                    "test.event".to_string(),
                    format!("e-{i}"),
                    "alice".to_string(),
                    serde_json::json!({"i": i}),
                    None,
                )
                .unwrap();
                store.ingest(&event).unwrap();
            }
            store.flush_storage().unwrap();
        }

        // Session 2: fresh boot. Events on disk, nothing in memory.
        let store = EventStore::with_config(EventStoreConfig::with_persistence(&storage_dir));
        assert_eq!(
            store.stats().total_events,
            0,
            "boot must be O(1) — no Parquet pre-load"
        );
        assert!(!store.is_tenant_loaded("alice"));
        assert!(!store.is_tenant_loaded("bob"));

        // First query for alice: triggers ensure_tenant_loaded.
        let results = store
            .query(&QueryEventsRequest {
                entity_id: None,
                event_type: None,
                tenant_id: Some("alice".to_string()),
                as_of: None,
                since: None,
                until: None,
                limit: None,
                event_type_prefix: None,
                exclude_event_type_prefix: None,
                payload_filter: None,
            })
            .unwrap();
        assert_eq!(results.len(), 3, "alice's 3 events are returned");
        assert!(store.is_tenant_loaded("alice"), "alice now warm");
        // bob untouched — load is per-tenant, so a query for alice
        // must not have hydrated bob.
        assert!(!store.is_tenant_loaded("bob"), "bob still cold");
    }

    #[test]
    fn test_query_invalid_tenant_id_returns_error_no_hang() {
        // Step 2 acceptance criterion: in-flight load failures
        // surface as errors, not infinite hangs. Path-traversal
        // input fails fast at sanitization and propagates.
        let temp_dir = TempDir::new().unwrap();
        let store = EventStore::with_config(EventStoreConfig::with_persistence(temp_dir.path()));

        let result = store.query(&QueryEventsRequest {
            entity_id: None,
            event_type: None,
            tenant_id: Some("../etc".to_string()),
            as_of: None,
            since: None,
            until: None,
            limit: None,
            event_type_prefix: None,
            exclude_event_type_prefix: None,
            payload_filter: None,
        });
        assert!(result.is_err(), "unsafe tenant_id must surface as error");
    }

    #[test]
    fn test_query_concurrent_first_queries_for_same_tenant_all_succeed() {
        // Singleflight: N threads racing to query the same cold
        // tenant must all return the same correct result. The
        // tenant-load must happen exactly once (verified
        // structurally by the per-tenant Mutex in tenant_loader,
        // tested directly in test_singleflight_blocks_second_caller).
        // This integration test confirms the wiring at the query
        // level — no thread observes a half-loaded state.
        let temp_dir = TempDir::new().unwrap();
        let storage_dir = temp_dir.path().to_path_buf();

        // Persist 25 events for tenant "alice".
        {
            let store = EventStore::with_config(EventStoreConfig::with_persistence(&storage_dir));
            for i in 0..25 {
                let event = Event::from_strings(
                    "test.event".to_string(),
                    format!("e-{i}"),
                    "alice".to_string(),
                    serde_json::json!({"i": i}),
                    None,
                )
                .unwrap();
                store.ingest(&event).unwrap();
            }
            store.flush_storage().unwrap();
        }

        // Fresh boot, then 8 threads simultaneously query alice.
        let store = Arc::new(EventStore::with_config(EventStoreConfig::with_persistence(
            &storage_dir,
        )));
        assert!(!store.is_tenant_loaded("alice"));

        let mut handles = Vec::new();
        for _ in 0..8 {
            let s = store.clone();
            handles.push(std::thread::spawn(move || {
                s.query(&QueryEventsRequest {
                    entity_id: None,
                    event_type: None,
                    tenant_id: Some("alice".to_string()),
                    as_of: None,
                    since: None,
                    until: None,
                    limit: None,
                    event_type_prefix: None,
                    exclude_event_type_prefix: None,
                    payload_filter: None,
                })
            }));
        }

        for h in handles {
            let result = h.join().unwrap().unwrap();
            assert_eq!(
                result.len(),
                25,
                "every concurrent caller must see all 25 events"
            );
        }
        assert!(store.is_tenant_loaded("alice"));
        // Memory has exactly 25 events — no double-load.
        assert_eq!(store.stats().total_events, 25);
    }

    #[test]
    fn test_query_two_cold_tenants_load_independently() {
        // Querying tenant A loads only A; querying B then loads
        // only B. State after both queries: both tenants warm,
        // memory has exactly the expected event counts.
        let temp_dir = TempDir::new().unwrap();
        let storage_dir = temp_dir.path().to_path_buf();

        {
            let store = EventStore::with_config(EventStoreConfig::with_persistence(&storage_dir));
            for i in 0..3 {
                store
                    .ingest(
                        &Event::from_strings(
                            "test.event".to_string(),
                            format!("a-{i}"),
                            "alice".to_string(),
                            serde_json::json!({"i": i}),
                            None,
                        )
                        .unwrap(),
                    )
                    .unwrap();
            }
            for i in 0..5 {
                store
                    .ingest(
                        &Event::from_strings(
                            "test.event".to_string(),
                            format!("b-{i}"),
                            "bob".to_string(),
                            serde_json::json!({"i": i}),
                            None,
                        )
                        .unwrap(),
                    )
                    .unwrap();
            }
            store.flush_storage().unwrap();
        }

        let store = EventStore::with_config(EventStoreConfig::with_persistence(&storage_dir));
        assert_eq!(store.stats().total_events, 0);

        // Query alice — bob stays cold.
        let alice = store
            .query(&QueryEventsRequest {
                entity_id: None,
                event_type: None,
                tenant_id: Some("alice".to_string()),
                as_of: None,
                since: None,
                until: None,
                limit: None,
                event_type_prefix: None,
                exclude_event_type_prefix: None,
                payload_filter: None,
            })
            .unwrap();
        assert_eq!(alice.len(), 3);
        assert!(store.is_tenant_loaded("alice"));
        assert!(!store.is_tenant_loaded("bob"));
        assert_eq!(store.stats().total_events, 3);

        // Query bob — both warm now.
        let bob = store
            .query(&QueryEventsRequest {
                entity_id: None,
                event_type: None,
                tenant_id: Some("bob".to_string()),
                as_of: None,
                since: None,
                until: None,
                limit: None,
                event_type_prefix: None,
                exclude_event_type_prefix: None,
                payload_filter: None,
            })
            .unwrap();
        assert_eq!(bob.len(), 5);
        assert!(store.is_tenant_loaded("bob"));
        assert_eq!(store.stats().total_events, 8);
    }

    #[test]
    fn test_boot_with_persisted_data_is_o1() {
        // Step 2's headline acceptance criterion: boot time does
        // not scale with persisted-data size. The 5M-events / <2s
        // target is too large for a unit test, so this asserts the
        // weaker but structural property: boot reads zero events
        // into memory regardless of how many are on disk.
        //
        // We persist 50 events across 3 tenants in session 1,
        // restart in session 2, and verify session 2's
        // total_events is 0. The actual boot wall-clock isn't
        // asserted here — it's machine-dependent — but the absence
        // of any in-memory data is the structural proxy that the
        // boot path no longer iterates Parquet.
        let temp_dir = TempDir::new().unwrap();
        let storage_dir = temp_dir.path().to_path_buf();

        {
            let store = EventStore::with_config(EventStoreConfig::with_persistence(&storage_dir));
            for tenant in ["alice", "bob", "carol"] {
                for i in 0..50 / 3 {
                    store
                        .ingest(
                            &Event::from_strings(
                                "test.event".to_string(),
                                format!("{tenant}-{i}"),
                                tenant.to_string(),
                                serde_json::json!({"i": i}),
                                None,
                            )
                            .unwrap(),
                        )
                        .unwrap();
                }
            }
            store.flush_storage().unwrap();
        }

        // Confirm there is in fact data on disk to load.
        let on_disk = find_parquet_files(&storage_dir);
        assert!(
            !on_disk.is_empty(),
            "session 1 should have produced parquet files; pre-condition for the test"
        );

        let started = std::time::Instant::now();
        let store = EventStore::with_config(EventStoreConfig::with_persistence(&storage_dir));
        let boot_elapsed = started.elapsed();

        assert_eq!(
            store.stats().total_events,
            0,
            "boot must not pre-load any Parquet events"
        );

        // Sanity: even on a slow CI box, an O(1) boot finishes in
        // well under a second. If this trips it's a strong signal
        // the boot path regressed to scanning the Parquet tree.
        assert!(
            boot_elapsed < std::time::Duration::from_secs(2),
            "boot took {boot_elapsed:?} — Step 2 boot should be O(1)"
        );
    }

    #[test]
    fn test_query_warm_tenant_does_not_re_read_disk() {
        // Performance contract: a warm tenant query goes through the
        // DashMap fast path. We can't easily assert "no disk read"
        // directly in a unit test, but we CAN assert the call
        // succeeds in O(in-memory-events) time even after the
        // on-disk file is removed — proving we didn't re-walk it.
        let temp_dir = TempDir::new().unwrap();
        let storage_dir = temp_dir.path().to_path_buf();

        let store = EventStore::with_config(EventStoreConfig::with_persistence(&storage_dir));
        for i in 0..3 {
            let event = Event::from_strings(
                "test.event".to_string(),
                format!("e-{i}"),
                "alice".to_string(),
                serde_json::json!({"i": i}),
                None,
            )
            .unwrap();
            store.ingest(&event).unwrap();
        }
        store.flush_storage().unwrap();

        // First query: cold, hits disk.
        let _ = store
            .query(&QueryEventsRequest {
                entity_id: None,
                event_type: None,
                tenant_id: Some("alice".to_string()),
                as_of: None,
                since: None,
                until: None,
                limit: None,
                event_type_prefix: None,
                exclude_event_type_prefix: None,
                payload_filter: None,
            })
            .unwrap();
        assert!(store.is_tenant_loaded("alice"));

        // Now wipe the on-disk file. A warm-path query must still
        // succeed because it doesn't need disk.
        let parquet_files = find_parquet_files(&storage_dir);
        for f in parquet_files {
            std::fs::remove_file(&f).unwrap();
        }

        let results = store
            .query(&QueryEventsRequest {
                entity_id: None,
                event_type: None,
                tenant_id: Some("alice".to_string()),
                as_of: None,
                since: None,
                until: None,
                limit: None,
                event_type_prefix: None,
                exclude_event_type_prefix: None,
                payload_filter: None,
            })
            .unwrap();
        assert_eq!(
            results.len(),
            3,
            "warm tenant query must not need disk; got {} events from a deleted parquet",
            results.len()
        );
    }

    #[test]
    fn test_event_store_default() {
        let store = EventStore::default();
        assert_eq!(store.stats().total_events, 0);
    }

    #[test]
    fn test_ingest_single_event() {
        let store = EventStore::new();
        let event = create_test_event("entity-1", "user.created");

        store.ingest(&event).unwrap();

        assert_eq!(store.stats().total_events, 1);
        assert_eq!(store.stats().total_ingested, 1);
    }

    #[test]
    fn test_ingest_multiple_events() {
        let store = EventStore::new();

        for i in 0..10 {
            let event = create_test_event(&format!("entity-{i}"), "user.created");
            store.ingest(&event).unwrap();
        }

        assert_eq!(store.stats().total_events, 10);
        assert_eq!(store.stats().total_ingested, 10);
    }

    #[test]
    fn test_query_by_entity_id() {
        let store = EventStore::new();

        store
            .ingest(&create_test_event("entity-1", "user.created"))
            .unwrap();
        store
            .ingest(&create_test_event("entity-2", "user.created"))
            .unwrap();
        store
            .ingest(&create_test_event("entity-1", "user.updated"))
            .unwrap();

        let results = store
            .query(&QueryEventsRequest {
                entity_id: Some("entity-1".to_string()),
                event_type: None,
                tenant_id: None,
                as_of: None,
                since: None,
                until: None,
                limit: None,
                event_type_prefix: None,
                exclude_event_type_prefix: None,
                payload_filter: None,
            })
            .unwrap();

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_query_by_event_type() {
        let store = EventStore::new();

        store
            .ingest(&create_test_event("entity-1", "user.created"))
            .unwrap();
        store
            .ingest(&create_test_event("entity-2", "user.updated"))
            .unwrap();
        store
            .ingest(&create_test_event("entity-3", "user.created"))
            .unwrap();

        let results = store
            .query(&QueryEventsRequest {
                entity_id: None,
                event_type: Some("user.created".to_string()),
                tenant_id: None,
                as_of: None,
                since: None,
                until: None,
                limit: None,
                event_type_prefix: None,
                exclude_event_type_prefix: None,
                payload_filter: None,
            })
            .unwrap();

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_query_with_limit() {
        let store = EventStore::new();

        for i in 0..10 {
            let event = create_test_event(&format!("entity-{i}"), "user.created");
            store.ingest(&event).unwrap();
        }

        let results = store
            .query(&QueryEventsRequest {
                entity_id: None,
                event_type: None,
                tenant_id: None,
                as_of: None,
                since: None,
                until: None,
                limit: Some(5),
                event_type_prefix: None,
                exclude_event_type_prefix: None,
                payload_filter: None,
            })
            .unwrap();

        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_query_empty_store() {
        let store = EventStore::new();

        let results = store
            .query(&QueryEventsRequest {
                entity_id: Some("non-existent".to_string()),
                event_type: None,
                tenant_id: None,
                as_of: None,
                since: None,
                until: None,
                limit: None,
                event_type_prefix: None,
                exclude_event_type_prefix: None,
                payload_filter: None,
            })
            .unwrap();

        assert!(results.is_empty());
    }

    #[test]
    fn test_reconstruct_state() {
        let store = EventStore::new();

        store
            .ingest(&create_test_event("entity-1", "user.created"))
            .unwrap();

        let state = store.reconstruct_state("entity-1", None).unwrap();
        // The state is wrapped with metadata
        assert_eq!(state["current_state"]["name"], "Test");
        assert_eq!(state["current_state"]["value"], 42);
    }

    #[test]
    fn test_reconstruct_state_not_found() {
        let store = EventStore::new();

        let result = store.reconstruct_state("non-existent", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_snapshot_empty() {
        let store = EventStore::new();

        let result = store.get_snapshot("non-existent");
        // Entity not found error is expected
        assert!(result.is_err());
    }

    #[test]
    fn test_create_snapshot() {
        let store = EventStore::new();

        store
            .ingest(&create_test_event("entity-1", "user.created"))
            .unwrap();

        store.create_snapshot("entity-1").unwrap();

        // Verify snapshot was created
        let snapshot = store.get_snapshot("entity-1").unwrap();
        assert_ne!(snapshot, serde_json::json!(null));
    }

    #[test]
    fn test_create_snapshot_entity_not_found() {
        let store = EventStore::new();

        let result = store.create_snapshot("non-existent");
        assert!(result.is_err());
    }

    #[test]
    fn test_websocket_manager() {
        let store = EventStore::new();
        let manager = store.websocket_manager();
        // Manager should be accessible
        assert!(Arc::strong_count(&manager) >= 1);
    }

    #[test]
    fn test_snapshot_manager() {
        let store = EventStore::new();
        let manager = store.snapshot_manager();
        assert!(Arc::strong_count(&manager) >= 1);
    }

    #[test]
    fn test_compaction_manager_none() {
        let store = EventStore::new();
        // Without storage_dir, compaction manager should be None
        assert!(store.compaction_manager().is_none());
    }

    #[test]
    fn test_schema_registry() {
        let store = EventStore::new();
        let registry = store.schema_registry();
        assert!(Arc::strong_count(&registry) >= 1);
    }

    #[test]
    fn test_replay_manager() {
        let store = EventStore::new();
        let manager = store.replay_manager();
        assert!(Arc::strong_count(&manager) >= 1);
    }

    #[test]
    fn test_pipeline_manager() {
        let store = EventStore::new();
        let manager = store.pipeline_manager();
        assert!(Arc::strong_count(&manager) >= 1);
    }

    #[test]
    fn test_projection_manager() {
        let store = EventStore::new();
        let manager = store.projection_manager();
        // Built-in projections should be registered
        let projections = manager.list_projections();
        assert!(projections.len() >= 2); // entity_snapshots and event_counters
    }

    #[test]
    fn test_projection_state_cache() {
        let store = EventStore::new();
        let cache = store.projection_state_cache();

        cache.insert("test:key".to_string(), serde_json::json!({"value": 123}));
        assert_eq!(cache.len(), 1);

        let value = cache.get("test:key").unwrap();
        assert_eq!(value["value"], 123);
    }

    #[test]
    fn test_metrics() {
        let store = EventStore::new();
        let metrics = store.metrics();
        assert!(Arc::strong_count(&metrics) >= 1);
    }

    #[test]
    fn test_store_stats() {
        let store = EventStore::new();

        store
            .ingest(&create_test_event("entity-1", "user.created"))
            .unwrap();
        store
            .ingest(&create_test_event("entity-2", "order.placed"))
            .unwrap();

        let stats = store.stats();
        assert_eq!(stats.total_events, 2);
        assert_eq!(stats.total_entities, 2);
        assert_eq!(stats.total_event_types, 2);
        assert_eq!(stats.total_ingested, 2);
    }

    #[test]
    fn test_event_store_config_default() {
        let config = EventStoreConfig::default();
        assert!(config.storage_dir.is_none());
        assert!(config.wal_dir.is_none());
    }

    #[test]
    fn test_event_store_config_with_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let config = EventStoreConfig::with_persistence(temp_dir.path());

        assert!(config.storage_dir.is_some());
        assert!(config.wal_dir.is_none());
    }

    #[test]
    fn test_event_store_config_with_wal() {
        let temp_dir = TempDir::new().unwrap();
        let config = EventStoreConfig::with_wal(temp_dir.path(), WALConfig::default());

        assert!(config.storage_dir.is_none());
        assert!(config.wal_dir.is_some());
    }

    #[test]
    fn test_event_store_config_with_all() {
        let temp_dir = TempDir::new().unwrap();
        let config = EventStoreConfig::with_all(temp_dir.path(), SnapshotConfig::default());

        assert!(config.storage_dir.is_some());
    }

    #[test]
    fn test_event_store_config_production() {
        let storage_dir = TempDir::new().unwrap();
        let wal_dir = TempDir::new().unwrap();
        let config = EventStoreConfig::production(
            storage_dir.path(),
            wal_dir.path(),
            SnapshotConfig::default(),
            WALConfig::default(),
            CompactionConfig::default(),
        );

        assert!(config.storage_dir.is_some());
        assert!(config.wal_dir.is_some());
    }

    // -----------------------------------------------------------------------
    // from_env_vars tests — verifies the env-var-to-config wiring that
    // caused the durability bug (events lost on restart) in v0.10.3.
    // -----------------------------------------------------------------------

    #[test]
    fn test_from_env_vars_data_dir_enables_full_persistence() {
        let (config, mode) = EventStoreConfig::from_env_vars(
            Some("/app/data".to_string()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(mode, "wal+parquet");
        assert_eq!(
            config.storage_dir.unwrap().to_str().unwrap(),
            "/app/data/storage"
        );
        assert_eq!(config.wal_dir.unwrap().to_str().unwrap(), "/app/data/wal");
    }

    #[test]
    fn test_from_env_vars_explicit_dirs() {
        let (config, mode) = EventStoreConfig::from_env_vars(
            None,
            Some("/custom/storage".to_string()),
            Some("/custom/wal".to_string()),
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(mode, "wal+parquet");
        assert_eq!(
            config.storage_dir.unwrap().to_str().unwrap(),
            "/custom/storage"
        );
        assert_eq!(config.wal_dir.unwrap().to_str().unwrap(), "/custom/wal");
    }

    #[test]
    fn test_from_env_vars_wal_disabled() {
        let (config, mode) = EventStoreConfig::from_env_vars(
            Some("/app/data".to_string()),
            None,
            None,
            Some("false".to_string()),
            None,
            None,
            None,
            None,
        );
        assert_eq!(mode, "parquet-only");
        assert!(config.storage_dir.is_some());
        assert!(config.wal_dir.is_none());
    }

    #[test]
    fn test_from_env_vars_no_dirs_is_in_memory() {
        let (config, mode) =
            EventStoreConfig::from_env_vars(None, None, None, None, None, None, None, None);
        assert_eq!(mode, "in-memory");
        assert!(config.storage_dir.is_none());
        assert!(config.wal_dir.is_none());
    }

    #[test]
    fn test_from_env_vars_empty_strings_treated_as_none() {
        let (_, mode) = EventStoreConfig::from_env_vars(
            Some(String::new()),
            Some(String::new()),
            Some(String::new()),
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(mode, "in-memory");
    }

    #[test]
    fn test_from_env_vars_explicit_overrides_data_dir() {
        let (config, mode) = EventStoreConfig::from_env_vars(
            Some("/app/data".to_string()),
            Some("/override/storage".to_string()),
            Some("/override/wal".to_string()),
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(mode, "wal+parquet");
        assert_eq!(
            config.storage_dir.unwrap().to_str().unwrap(),
            "/override/storage"
        );
        assert_eq!(config.wal_dir.unwrap().to_str().unwrap(), "/override/wal");
    }

    #[test]
    fn test_from_env_vars_wal_only() {
        let (config, mode) = EventStoreConfig::from_env_vars(
            None,
            None,
            Some("/wal/only".to_string()),
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(mode, "wal-only");
        assert!(config.storage_dir.is_none());
        assert_eq!(config.wal_dir.unwrap().to_str().unwrap(), "/wal/only");
    }

    #[test]
    fn test_from_env_vars_cache_bytes_parses_decimal() {
        let (config, _) = EventStoreConfig::from_env_vars(
            Some("/app/data".to_string()),
            None,
            None,
            None,
            Some("536870912".to_string()),
            // 512 MiB
            None,
            None,
            None,
        );
        assert_eq!(config.cache_byte_budget, Some(536_870_912));
    }

    #[test]
    fn test_from_env_vars_cache_bytes_unparseable_disables_budget() {
        // Garbage in CACHE_BYTES doesn't fail boot — we log and
        // fall back to no-budget. The unbounded fallback is safe
        // (just the pre-Step-3 behavior).
        let (config, _) = EventStoreConfig::from_env_vars(
            Some("/app/data".to_string()),
            None,
            None,
            None,
            Some("not-a-number".to_string()),
            None,
            None,
            None,
        );
        assert_eq!(config.cache_byte_budget, None);
    }

    #[test]
    fn test_from_env_vars_cache_bytes_empty_disables_budget() {
        let (config, _) = EventStoreConfig::from_env_vars(
            Some("/app/data".to_string()),
            None,
            None,
            None,
            Some(String::new()),
            None,
            None,
            None,
        );
        assert_eq!(config.cache_byte_budget, None);
    }

    #[test]
    fn test_from_env_vars_snapshot_interval_overrides_default() {
        // ALLSOURCE_SNAPSHOT_INTERVAL_SECONDS plumbs through to
        // CompactionConfig.compaction_interval_seconds. Default is
        // 3600s (hourly) per the bead.
        let (config, _) = EventStoreConfig::from_env_vars(
            Some("/app/data".to_string()),
            None,
            None,
            None,
            None,
            Some("60".to_string()),
            None,
            None,
        );
        assert_eq!(config.compaction_config.compaction_interval_seconds, 60);
    }

    #[test]
    fn test_from_env_vars_snapshot_interval_default_is_hourly() {
        let (config, _) = EventStoreConfig::from_env_vars(
            Some("/app/data".to_string()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(config.compaction_config.compaction_interval_seconds, 3600);
    }

    #[test]
    fn test_from_env_vars_snapshot_interval_unparseable_falls_back() {
        let (config, _) = EventStoreConfig::from_env_vars(
            Some("/app/data".to_string()),
            None,
            None,
            None,
            None,
            Some("not-a-number".to_string()),
            None,
            None,
        );
        assert_eq!(config.compaction_config.compaction_interval_seconds, 3600);
    }

    #[test]
    fn test_from_env_vars_retention_system_days_overrides_default() {
        // Step 5: ALLSOURCE_RETENTION_SYSTEM_DAYS overrides the
        // default 30-day TTL for the system tenant.
        let (config, _) = EventStoreConfig::from_env_vars(
            Some("/app/data".to_string()),
            None,
            None,
            None,
            None,
            None,
            Some("7".to_string()),
            None,
        );
        let ttl = config
            .compaction_config
            .retention
            .ttl_for("system")
            .unwrap();
        assert_eq!(ttl.as_secs(), 7 * 24 * 3600);
    }

    #[test]
    fn test_from_env_vars_retention_default_is_30_days_for_system() {
        let (config, _) = EventStoreConfig::from_env_vars(
            Some("/app/data".to_string()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        let ttl = config
            .compaction_config
            .retention
            .ttl_for("system")
            .unwrap();
        assert_eq!(ttl.as_secs(), 30 * 24 * 3600);
        // Other tenants keep forever by default.
        assert!(config.compaction_config.retention.ttl_for("acme").is_none());
    }

    #[test]
    fn test_store_stats_serde() {
        let stats = StoreStats {
            total_events: 100,
            total_entities: 50,
            total_event_types: 10,
            total_ingested: 100,
        };

        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("\"total_events\":100"));
        assert!(json.contains("\"total_entities\":50"));
    }

    #[test]
    fn test_query_with_entity_and_type() {
        let store = EventStore::new();

        store
            .ingest(&create_test_event("entity-1", "user.created"))
            .unwrap();
        store
            .ingest(&create_test_event("entity-1", "user.updated"))
            .unwrap();
        store
            .ingest(&create_test_event("entity-2", "user.created"))
            .unwrap();

        let results = store
            .query(&QueryEventsRequest {
                entity_id: Some("entity-1".to_string()),
                event_type: Some("user.created".to_string()),
                tenant_id: None,
                as_of: None,
                since: None,
                until: None,
                limit: None,
                event_type_prefix: None,
                exclude_event_type_prefix: None,
                payload_filter: None,
            })
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].event_type_str(), "user.created");
    }

    #[test]
    fn test_query_by_event_type_prefix() {
        let store = EventStore::new();

        // Ingest events with various types
        store
            .ingest(&create_test_event("entity-1", "index.created"))
            .unwrap();
        store
            .ingest(&create_test_event("entity-2", "index.updated"))
            .unwrap();
        store
            .ingest(&create_test_event("entity-3", "trade.created"))
            .unwrap();
        store
            .ingest(&create_test_event("entity-4", "trade.completed"))
            .unwrap();
        store
            .ingest(&create_test_event("entity-5", "balance.updated"))
            .unwrap();

        // Query with prefix "index." should return exactly 2
        let results = store
            .query(&QueryEventsRequest {
                entity_id: None,
                event_type: None,
                tenant_id: None,
                as_of: None,
                since: None,
                until: None,
                limit: None,
                event_type_prefix: Some("index.".to_string()),
                exclude_event_type_prefix: None,
                payload_filter: None,
            })
            .unwrap();

        assert_eq!(results.len(), 2);
        assert!(
            results
                .iter()
                .all(|e| e.event_type_str().starts_with("index."))
        );
    }

    #[test]
    fn test_query_by_event_type_prefix_empty_returns_all() {
        let store = EventStore::new();

        store
            .ingest(&create_test_event("entity-1", "index.created"))
            .unwrap();
        store
            .ingest(&create_test_event("entity-2", "trade.created"))
            .unwrap();

        // Empty prefix matches all types
        let results = store
            .query(&QueryEventsRequest {
                entity_id: None,
                event_type: None,
                tenant_id: None,
                as_of: None,
                since: None,
                until: None,
                limit: None,
                event_type_prefix: Some(String::new()),
                exclude_event_type_prefix: None,
                payload_filter: None,
            })
            .unwrap();

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_query_by_event_type_prefix_no_match() {
        let store = EventStore::new();

        store
            .ingest(&create_test_event("entity-1", "index.created"))
            .unwrap();

        let results = store
            .query(&QueryEventsRequest {
                entity_id: None,
                event_type: None,
                tenant_id: None,
                as_of: None,
                since: None,
                until: None,
                limit: None,
                event_type_prefix: Some("nonexistent.".to_string()),
                exclude_event_type_prefix: None,
                payload_filter: None,
            })
            .unwrap();

        assert!(results.is_empty());
    }

    #[test]
    fn test_query_by_entity_with_type_prefix() {
        let store = EventStore::new();

        store
            .ingest(&create_test_event("entity-1", "index.created"))
            .unwrap();
        store
            .ingest(&create_test_event("entity-1", "trade.created"))
            .unwrap();
        store
            .ingest(&create_test_event("entity-2", "index.updated"))
            .unwrap();

        // Query entity-1 with prefix "index." should return 1
        let results = store
            .query(&QueryEventsRequest {
                entity_id: Some("entity-1".to_string()),
                event_type: None,
                tenant_id: None,
                as_of: None,
                since: None,
                until: None,
                limit: None,
                event_type_prefix: Some("index.".to_string()),
                exclude_event_type_prefix: None,
                payload_filter: None,
            })
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].event_type_str(), "index.created");
    }

    #[test]
    fn test_query_prefix_with_limit() {
        let store = EventStore::new();

        for i in 0..5 {
            store
                .ingest(&create_test_event(&format!("entity-{i}"), "index.created"))
                .unwrap();
        }

        let results = store
            .query(&QueryEventsRequest {
                entity_id: None,
                event_type: None,
                tenant_id: None,
                as_of: None,
                since: None,
                until: None,
                limit: Some(3),
                event_type_prefix: Some("index.".to_string()),
                exclude_event_type_prefix: None,
                payload_filter: None,
            })
            .unwrap();

        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_query_prefix_alongside_existing_filters() {
        let store = EventStore::new();

        store
            .ingest(&create_test_event("entity-1", "index.created"))
            .unwrap();
        // Sleep briefly to ensure different timestamps
        std::thread::sleep(std::time::Duration::from_millis(10));
        store
            .ingest(&create_test_event("entity-2", "index.strategy.updated"))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        store
            .ingest(&create_test_event("entity-3", "index.deleted"))
            .unwrap();

        // Prefix with limit
        let results = store
            .query(&QueryEventsRequest {
                entity_id: None,
                event_type: None,
                tenant_id: None,
                as_of: None,
                since: None,
                until: None,
                limit: Some(2),
                event_type_prefix: Some("index.".to_string()),
                exclude_event_type_prefix: None,
                payload_filter: None,
            })
            .unwrap();

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_query_with_payload_filter() {
        let store = EventStore::new();

        // Ingest 5 events with user_id=alice
        for i in 0..5 {
            store
                .ingest(&create_test_event_with_payload(
                    &format!("entity-{i}"),
                    "user.action",
                    serde_json::json!({"user_id": "alice", "action": "click"}),
                ))
                .unwrap();
        }
        // Ingest 5 events with user_id=bob
        for i in 5..10 {
            store
                .ingest(&create_test_event_with_payload(
                    &format!("entity-{i}"),
                    "user.action",
                    serde_json::json!({"user_id": "bob", "action": "view"}),
                ))
                .unwrap();
        }

        // Filter for alice
        let results = store
            .query(&QueryEventsRequest {
                entity_id: None,
                event_type: Some("user.action".to_string()),
                tenant_id: None,
                as_of: None,
                since: None,
                until: None,
                limit: None,
                event_type_prefix: None,
                exclude_event_type_prefix: None,
                payload_filter: Some(r#"{"user_id":"alice"}"#.to_string()),
            })
            .unwrap();

        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_query_payload_filter_non_existent_field() {
        let store = EventStore::new();

        store
            .ingest(&create_test_event_with_payload(
                "entity-1",
                "user.action",
                serde_json::json!({"user_id": "alice"}),
            ))
            .unwrap();

        // Filter for a field that doesn't exist — returns 0, not error
        let results = store
            .query(&QueryEventsRequest {
                entity_id: None,
                event_type: None,
                tenant_id: None,
                as_of: None,
                since: None,
                until: None,
                limit: None,
                event_type_prefix: None,
                exclude_event_type_prefix: None,
                payload_filter: Some(r#"{"nonexistent":"value"}"#.to_string()),
            })
            .unwrap();

        assert!(results.is_empty());
    }

    #[test]
    fn test_query_payload_filter_with_prefix() {
        let store = EventStore::new();

        store
            .ingest(&create_test_event_with_payload(
                "entity-1",
                "index.created",
                serde_json::json!({"status": "active"}),
            ))
            .unwrap();
        store
            .ingest(&create_test_event_with_payload(
                "entity-2",
                "index.created",
                serde_json::json!({"status": "inactive"}),
            ))
            .unwrap();
        store
            .ingest(&create_test_event_with_payload(
                "entity-3",
                "trade.created",
                serde_json::json!({"status": "active"}),
            ))
            .unwrap();

        // Combine prefix + payload filter
        let results = store
            .query(&QueryEventsRequest {
                entity_id: None,
                event_type: None,
                tenant_id: None,
                as_of: None,
                since: None,
                until: None,
                limit: None,
                event_type_prefix: Some("index.".to_string()),
                exclude_event_type_prefix: None,
                payload_filter: Some(r#"{"status":"active"}"#.to_string()),
            })
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entity_id().to_string(), "entity-1");
    }

    #[test]
    fn test_flush_storage_no_storage() {
        let store = EventStore::new();
        // Without storage, flush should succeed (no-op)
        let result = store.flush_storage();
        assert!(result.is_ok());
    }

    #[test]
    fn test_state_evolution() {
        let store = EventStore::new();

        // Initial state
        store
            .ingest(
                &Event::from_strings(
                    "user.created".to_string(),
                    "user-1".to_string(),
                    "default".to_string(),
                    serde_json::json!({"name": "Alice", "age": 25}),
                    None,
                )
                .unwrap(),
            )
            .unwrap();

        // Update state
        store
            .ingest(
                &Event::from_strings(
                    "user.updated".to_string(),
                    "user-1".to_string(),
                    "default".to_string(),
                    serde_json::json!({"age": 26}),
                    None,
                )
                .unwrap(),
            )
            .unwrap();

        let state = store.reconstruct_state("user-1", None).unwrap();
        // The state is wrapped with metadata
        assert_eq!(state["current_state"]["name"], "Alice");
        assert_eq!(state["current_state"]["age"], 26);
    }

    #[test]
    fn test_reject_system_event_types() {
        let store = EventStore::new();

        // System event types should be rejected via user-facing ingestion
        let event = Event::reconstruct_from_strings(
            uuid::Uuid::new_v4(),
            "_system.tenant.created".to_string(),
            "_system:tenant:acme".to_string(),
            "_system".to_string(),
            serde_json::json!({"name": "ACME"}),
            chrono::Utc::now(),
            None,
            1,
        );

        let result = store.ingest(&event);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("reserved for internal use"),
            "Expected system namespace rejection, got: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // Crash recovery: WAL events survive restart via Parquet checkpoint.
    // Regression test for GitHub issue #84 — flush_storage() was a no-op
    // during recovery because events were never buffered into Parquet's
    // current_batch before flushing.
    // -----------------------------------------------------------------------

    #[test]
    fn test_wal_recovery_checkpoints_to_parquet() {
        let data_dir = TempDir::new().unwrap();
        let storage_dir = data_dir.path().join("storage");
        let wal_dir = data_dir.path().join("wal");

        // Session 1: ingest events with WAL + Parquet
        {
            let config = EventStoreConfig::production(
                &storage_dir,
                &wal_dir,
                SnapshotConfig::default(),
                WALConfig {
                    sync_on_write: true,
                    ..WALConfig::default()
                },
                CompactionConfig::default(),
            );
            let store = EventStore::with_config(config);

            for i in 0..5 {
                let event = Event::from_strings(
                    "test.created".to_string(),
                    format!("entity-{i}"),
                    "default".to_string(),
                    serde_json::json!({"index": i}),
                    None,
                )
                .unwrap();
                store.ingest(&event).unwrap();
            }

            assert_eq!(store.stats().total_events, 5);

            // Do NOT call flush_storage or shutdown — simulate a crash.
            // Events are in WAL (sync_on_write: true) but NOT in Parquet.
        }

        // Verify WAL file has data
        let wal_files: Vec<_> = std::fs::read_dir(&wal_dir)
            .unwrap()
            .filter_map(std::result::Result::ok)
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "log"))
            .collect();
        assert!(!wal_files.is_empty(), "WAL file should exist");
        let wal_size = wal_files[0].metadata().unwrap().len();
        assert!(wal_size > 0, "WAL file should have data (got 0 bytes)");

        // Session 2: reopen — recovery should checkpoint WAL to Parquet, then truncate
        {
            let config = EventStoreConfig::production(
                &storage_dir,
                &wal_dir,
                SnapshotConfig::default(),
                WALConfig {
                    sync_on_write: true,
                    ..WALConfig::default()
                },
                CompactionConfig::default(),
            );
            let store = EventStore::with_config(config);

            // Events should be recovered
            assert_eq!(
                store.stats().total_events,
                5,
                "Session 2 should have all 5 events after WAL recovery"
            );

            // Parquet should now have files (checkpoint happened).
            // After Step 1, files live under <root>/<tenant>/<yyyy-mm>/,
            // so walk recursively.
            let parquet_files = find_parquet_files(&storage_dir);
            assert!(
                !parquet_files.is_empty(),
                "Parquet file should exist after WAL checkpoint"
            );
        }

        // Session 3: reopen again — events should be reachable via
        // lazy-load (Step 2: boot does not pre-load Parquet).
        {
            let config = EventStoreConfig::production(
                &storage_dir,
                &wal_dir,
                SnapshotConfig::default(),
                WALConfig {
                    sync_on_write: true,
                    ..WALConfig::default()
                },
                CompactionConfig::default(),
            );
            let store = EventStore::with_config(config);

            // Boot is now O(1) — Parquet stays cold until first
            // per-tenant query. WAL was truncated in session 2,
            // so nothing is pre-loaded.
            assert_eq!(
                store.stats().total_events,
                0,
                "Session 3 boot should not pre-load Parquet (lazy-load mode)"
            );

            // Trigger lazy load for the test tenant (events were
            // ingested with tenant_id=\"default\").
            store.ensure_tenant_loaded("default").unwrap();
            assert_eq!(
                store.stats().total_events,
                5,
                "Session 3 should have all 5 events after ensure_tenant_loaded"
            );
        }
    }

    #[test]
    fn test_parquet_restore_surfaces_errors_not_silent() {
        // Write events with WAL+Parquet, flush to Parquet, then corrupt the
        // Parquet file. On reload, the error must be logged (not silently
        // swallowed as 0 events).
        let data_dir = TempDir::new().unwrap();
        let storage_dir = data_dir.path().join("storage");
        let wal_dir = data_dir.path().join("wal");

        // Session 1: write events and flush to Parquet
        {
            let config = EventStoreConfig::production(
                &storage_dir,
                &wal_dir,
                SnapshotConfig::default(),
                WALConfig {
                    sync_on_write: true,
                    ..WALConfig::default()
                },
                CompactionConfig::default(),
            );
            let store = EventStore::with_config(config);

            for i in 0..3 {
                let event = Event::from_strings(
                    "test.created".to_string(),
                    format!("entity-{i}"),
                    "default".to_string(),
                    serde_json::json!({"i": i}),
                    None,
                )
                .unwrap();
                store.ingest(&event).unwrap();
            }

            store.flush_storage().unwrap();
            assert_eq!(store.stats().total_events, 3);
        }

        // Verify parquet file exists. After Step 1 the file lives
        // under <root>/<tenant>/<yyyy-mm>/, so walk recursively.
        let parquet_files = find_parquet_files(&storage_dir);
        assert!(!parquet_files.is_empty(), "Parquet file must exist");

        // Corrupt the parquet file
        std::fs::write(&parquet_files[0], b"corrupted data").unwrap();

        // Truncate WAL so only Parquet matters
        for entry in std::fs::read_dir(&wal_dir).unwrap().flatten() {
            std::fs::write(entry.path(), b"").unwrap();
        }

        // Session 2: reload — should NOT silently report 0 events.
        // The error is logged via tracing::error! which we can't capture in a
        // unit test, but we CAN verify the store has 0 events (previously this
        // looked identical to "no data on disk" — now there's an error log).
        // The key behavioral change is that with_config no longer uses a
        // let-chain that silently drops the Err variant.
        {
            let config = EventStoreConfig::production(
                &storage_dir,
                &wal_dir,
                SnapshotConfig::default(),
                WALConfig::default(),
                CompactionConfig::default(),
            );
            let store = EventStore::with_config(config);

            // Store has 0 events because Parquet is corrupted — but the error
            // is now logged (not silently swallowed).
            assert_eq!(store.stats().total_events, 0);
        }
    }

    // -----------------------------------------------------------------------
    // Step 6: Bounded WAL replay. Each successful checkpoint truncates the
    // WAL so cold-start replay is O(one checkpoint interval) regardless of
    // total dataset size.
    // -----------------------------------------------------------------------

    /// Count entries in every WAL file under `wal_dir` (any line that
    /// parses as a valid JSON object — the line format is one
    /// JSON-serialized WALEntry per line, see WALFile::write_entry).
    fn count_wal_entries(wal_dir: &std::path::Path) -> usize {
        use std::io::{BufRead, BufReader};
        let mut total = 0usize;
        let Ok(entries) = std::fs::read_dir(wal_dir) else {
            return 0;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "log") {
                continue;
            }
            let Ok(file) = std::fs::File::open(&path) else {
                continue;
            };
            for line in BufReader::new(file)
                .lines()
                .map_while(std::result::Result::ok)
            {
                if !line.trim().is_empty() {
                    total += 1;
                }
            }
        }
        total
    }

    #[test]
    fn test_checkpoint_truncates_wal_after_flush() {
        // After a successful checkpoint, every previously-ingested event
        // should be in Parquet, and the WAL should be empty (truncated).
        // This is the load-bearing invariant for Step 6's bounded-replay
        // promise — without truncation, the WAL grows unboundedly.
        let data_dir = TempDir::new().unwrap();
        let storage_dir = data_dir.path().join("storage");
        let wal_dir = data_dir.path().join("wal");

        let config = EventStoreConfig::production(
            &storage_dir,
            &wal_dir,
            SnapshotConfig::default(),
            WALConfig {
                sync_on_write: true,
                ..WALConfig::default()
            },
            CompactionConfig::default(),
        );
        let store = EventStore::with_config(config);

        for i in 0..10 {
            let event = Event::from_strings(
                "test.created".to_string(),
                format!("entity-{i}"),
                "default".to_string(),
                serde_json::json!({"i": i}),
                None,
            )
            .unwrap();
            store.ingest(&event).unwrap();
        }

        // Sanity: all 10 events are in the WAL pre-checkpoint.
        assert_eq!(
            count_wal_entries(&wal_dir),
            10,
            "WAL should have 10 events before checkpoint"
        );

        store.checkpoint().unwrap();

        assert_eq!(
            count_wal_entries(&wal_dir),
            0,
            "WAL should be empty after successful checkpoint"
        );
        let parquet_files = find_parquet_files(&storage_dir);
        assert!(!parquet_files.is_empty(), "Parquet should hold the events");
    }

    #[test]
    fn test_replay_only_post_checkpoint_events_after_crash() {
        // Headline AC for the bead: write N events, checkpoint, write K
        // more, simulate a crash, restart, and verify only K events go
        // through replay (not N+K).
        //
        // Uses small N (50) and K (5) for test speed — the property
        // is the same as the spec's 1M+10k example, just scaled down.
        let data_dir = TempDir::new().unwrap();
        let storage_dir = data_dir.path().join("storage");
        let wal_dir = data_dir.path().join("wal");

        let config_factory = || {
            EventStoreConfig::production(
                &storage_dir,
                &wal_dir,
                SnapshotConfig::default(),
                WALConfig {
                    sync_on_write: true,
                    ..WALConfig::default()
                },
                CompactionConfig::default(),
            )
        };

        // Session 1: ingest N, checkpoint, ingest K, then drop without
        // a graceful shutdown — that's the crash.
        const N: usize = 50;
        const K: usize = 5;
        {
            let store = EventStore::with_config(config_factory());
            for i in 0..N {
                store
                    .ingest(
                        &Event::from_strings(
                            "pre.checkpoint".to_string(),
                            format!("e-{i}"),
                            "default".to_string(),
                            serde_json::json!({"i": i}),
                            None,
                        )
                        .unwrap(),
                    )
                    .unwrap();
            }
            store.checkpoint().unwrap();
            assert_eq!(
                count_wal_entries(&wal_dir),
                0,
                "WAL should be empty immediately after checkpoint"
            );

            for i in 0..K {
                store
                    .ingest(
                        &Event::from_strings(
                            "post.checkpoint".to_string(),
                            format!("p-{i}"),
                            "default".to_string(),
                            serde_json::json!({"i": i}),
                            None,
                        )
                        .unwrap(),
                    )
                    .unwrap();
            }
            assert_eq!(
                count_wal_entries(&wal_dir),
                K,
                "WAL should hold only post-checkpoint events"
            );
            // Drop without flushing — simulates a crash mid-write.
        }

        // Session 2: reopen. Recovery should replay only the K post-
        // checkpoint events from the WAL — the N pre-checkpoint events
        // are durable in Parquet and lazy-loaded on demand.
        {
            let store = EventStore::with_config(config_factory());
            // total_events reflects only WAL-recovered events at boot
            // (Step 2 — Parquet stays cold until first per-tenant
            // query). So the WAL replay size IS exactly K.
            assert_eq!(
                store.stats().total_events,
                K,
                "Boot should replay exactly K events from WAL (the post-checkpoint window), not N+K"
            );

            // Lazy-load brings the rest in.
            store.ensure_tenant_loaded("default").unwrap();
            assert_eq!(
                store.stats().total_events,
                N + K,
                "After lazy-load, both pre- and post-checkpoint events should be reachable"
            );
        }
    }

    #[test]
    fn test_checkpoint_is_idempotent() {
        // Calling checkpoint() twice in a row is safe: the second call
        // finds an empty WAL and an empty Parquet batch, and no-ops.
        let data_dir = TempDir::new().unwrap();
        let storage_dir = data_dir.path().join("storage");
        let wal_dir = data_dir.path().join("wal");

        let store = EventStore::with_config(EventStoreConfig::production(
            &storage_dir,
            &wal_dir,
            SnapshotConfig::default(),
            WALConfig::default(),
            CompactionConfig::default(),
        ));

        for i in 0..5 {
            store
                .ingest(
                    &Event::from_strings(
                        "x".to_string(),
                        format!("e-{i}"),
                        "default".to_string(),
                        serde_json::json!({}),
                        None,
                    )
                    .unwrap(),
                )
                .unwrap();
        }

        store.checkpoint().unwrap();
        // Second call is a no-op and must not error.
        store.checkpoint().unwrap();
        assert_eq!(count_wal_entries(&wal_dir), 0);
    }

    #[test]
    fn test_checkpoint_noop_in_memory_only_mode() {
        // Without WAL configured, checkpoint() is a no-op.
        let store = EventStore::new();
        store.checkpoint().unwrap();
    }

    #[test]
    fn test_checkpoint_interval_from_env_defaults_to_60s_when_wal_enabled() {
        let (config, _) = EventStoreConfig::from_env_vars(
            Some("/app/data".to_string()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(config.checkpoint_interval_secs, Some(60));
    }

    #[test]
    fn test_checkpoint_interval_from_env_overrides_default() {
        let (config, _) = EventStoreConfig::from_env_vars(
            Some("/app/data".to_string()),
            None,
            None,
            None,
            None,
            None,
            None,
            Some("15".to_string()),
        );
        assert_eq!(config.checkpoint_interval_secs, Some(15));
    }

    #[test]
    fn test_checkpoint_interval_disabled_when_wal_disabled() {
        // No WAL → no checkpoint loop, regardless of env var value.
        let (config, _) = EventStoreConfig::from_env_vars(
            Some("/app/data".to_string()),
            None,
            None,
            Some("false".to_string()),
            None,
            None,
            None,
            Some("15".to_string()),
        );
        assert_eq!(config.checkpoint_interval_secs, None);
    }

    #[test]
    fn test_checkpoint_interval_unparseable_falls_back_to_default() {
        let (config, _) = EventStoreConfig::from_env_vars(
            Some("/app/data".to_string()),
            None,
            None,
            None,
            None,
            None,
            None,
            Some("not-a-number".to_string()),
        );
        assert_eq!(config.checkpoint_interval_secs, Some(60));
    }

    // ── Tenant-scoped stats + state reconstruction (#230) ──────────────────
    //
    // `stats()` and `reconstruct_state()` are global. The gateway exposes the
    // scoped variants below to tenants, so cross-tenant isolation is the
    // property that matters most here.

    fn seed_two_tenants() -> EventStore {
        let store = EventStore::new();

        // alice: 3 events, 2 entities, 2 types
        for (entity, etype, payload) in [
            (
                "a-1",
                "created",
                serde_json::json!({"colour": "red", "size": 1}),
            ),
            ("a-1", "updated", serde_json::json!({"colour": "blue"})),
            ("a-2", "created", serde_json::json!({"colour": "green"})),
        ] {
            store
                .ingest(
                    &Event::from_strings(
                        etype.to_string(),
                        entity.to_string(),
                        "alice".to_string(),
                        payload,
                        None,
                    )
                    .unwrap(),
                )
                .unwrap();
        }

        // bob: 1 event, and it deliberately reuses alice's entity_id "a-1"
        store
            .ingest(
                &Event::from_strings(
                    "created".to_string(),
                    "a-1".to_string(),
                    "bob".to_string(),
                    serde_json::json!({"colour": "BOB_SECRET", "bob_only": true}),
                    None,
                )
                .unwrap(),
            )
            .unwrap();

        store
    }

    #[test]
    fn test_stats_for_tenant_counts_only_that_tenant() {
        let store = seed_two_tenants();

        let alice = store.stats_for_tenant("alice");
        assert_eq!(alice.total_events, 3);
        assert_eq!(alice.total_entities, 2);
        assert_eq!(alice.total_event_types, 2);
        assert_eq!(alice.event_types.get("created"), Some(&2));
        assert_eq!(alice.event_types.get("updated"), Some(&1));

        let bob = store.stats_for_tenant("bob");
        assert_eq!(bob.total_events, 1);
        assert_eq!(bob.total_entities, 1);
        assert_eq!(bob.total_event_types, 1);
    }

    #[test]
    fn test_stats_for_tenant_never_reports_global_totals() {
        let store = seed_two_tenants();

        // The global view sees all 4; neither tenant may.
        assert_eq!(store.stats().total_events, 4);
        assert_eq!(store.stats_for_tenant("alice").total_events, 3);
        assert_eq!(store.stats_for_tenant("bob").total_events, 1);

        // total_ingested is a global process counter, so the scoped form must
        // report the tenant's own total rather than leaking the fleet tally.
        assert_eq!(store.stats_for_tenant("bob").total_ingested, 1);
    }

    #[test]
    fn test_stats_for_tenant_unknown_tenant_is_empty_not_global() {
        let store = seed_two_tenants();
        let nobody = store.stats_for_tenant("does-not-exist");

        assert_eq!(nobody.total_events, 0);
        assert_eq!(nobody.total_entities, 0);
        assert!(nobody.event_types.is_empty());
        assert!(nobody.oldest_event.is_none());
        assert!(nobody.newest_event.is_none());
    }

    #[test]
    fn test_stats_for_tenant_reports_time_range() {
        let store = seed_two_tenants();
        let alice = store.stats_for_tenant("alice");

        let oldest = alice.oldest_event.expect("oldest");
        let newest = alice.newest_event.expect("newest");
        assert!(oldest <= newest);
    }

    #[test]
    fn test_reconstruct_state_for_tenant_isolates_shared_entity_id() {
        let store = seed_two_tenants();

        // Both tenants have an entity called "a-1". Each must see only its own.
        let alice = store
            .reconstruct_state_for_tenant("a-1", None, "alice")
            .unwrap();
        let alice_state = alice.get("current_state").unwrap();
        assert_eq!(alice_state.get("colour").unwrap(), "blue"); // last write wins
        assert_eq!(alice_state.get("size").unwrap(), 1);
        assert!(
            alice_state.get("bob_only").is_none(),
            "alice must not see bob's payload keys: {alice_state:?}"
        );
        assert_eq!(alice.get("event_count").unwrap(), 2);

        let bob = store
            .reconstruct_state_for_tenant("a-1", None, "bob")
            .unwrap();
        let bob_state = bob.get("current_state").unwrap();
        assert_eq!(bob_state.get("colour").unwrap(), "BOB_SECRET");
        assert_eq!(bob.get("event_count").unwrap(), 1);
    }

    #[test]
    fn test_reconstruct_state_for_tenant_rejects_another_tenants_entity() {
        let store = seed_two_tenants();

        // "a-2" belongs to alice only.
        assert!(
            store
                .reconstruct_state_for_tenant("a-2", None, "alice")
                .is_ok()
        );
        assert!(
            store
                .reconstruct_state_for_tenant("a-2", None, "bob")
                .is_err(),
            "bob must not be able to read alice's entity"
        );
    }

    #[test]
    fn test_global_reconstruct_state_still_spans_tenants() {
        // The unscoped form is unchanged — it is the internal/admin path and the
        // gateway never exposes it without a tenant_id.
        let store = seed_two_tenants();
        let all = store.reconstruct_state("a-1", None).unwrap();
        assert_eq!(all.get("event_count").unwrap(), 3);
    }

    // -----------------------------------------------------------------
    // Issue #251: `limit` must bound the store's work, not just its
    // response. These guards count `Event` CLONES (crate::clone_probe),
    // not returned rows — `query_results_total` is incremented with
    // `results.len()`, so it reads 1 both when the store clones one event
    // and when it clones the whole history and throws the rest away.
    // Revert `query_window` to "clone every match, sort all N, truncate"
    // and these go red; row-shaped assertions alone would not.
    // -----------------------------------------------------------------

    fn seed_hot_entity(history: usize) -> EventStore {
        let store = EventStore::new();
        for _ in 0..history {
            store
                .ingest(&create_test_event("entity-hot", "user.updated"))
                .unwrap();
        }
        store
    }

    fn hot_request(limit: Option<usize>) -> QueryEventsRequest {
        QueryEventsRequest {
            entity_id: Some("entity-hot".to_string()),
            limit,
            ..QueryEventsRequest::default()
        }
    }

    #[test]
    fn query_window_limit_bounds_materialization_not_just_the_response() {
        const HISTORY: usize = 400;
        let store = seed_hot_entity(HISTORY);

        // The documented "latest event for an entity" read.
        let ((page, total), materialized) = crate::clone_probe::measure(|| {
            store.query_window(&hot_request(Some(1)), 0, true).unwrap()
        });
        assert_eq!(page.len(), 1, "limit=1 returns one event");
        assert_eq!(total, HISTORY, "total is still the full match count");
        assert_eq!(
            materialized, 1,
            "limit=1 cloned {materialized} of {HISTORY} events: `limit` must \
             bound what the store materializes, not just what it returns"
        );

        // An offset page materializes the page, not everything up to it.
        let ((page, _), materialized) = crate::clone_probe::measure(|| {
            store
                .query_window(&hot_request(Some(5)), 300, false)
                .unwrap()
        });
        assert_eq!(page.len(), 5);
        assert_eq!(
            materialized, 5,
            "offset=300&limit=5 cloned {materialized} events, expected 5"
        );

        // The unbounded query legitimately materializes everything — the
        // ceiling only applies where the caller asked for a window.
        let ((all, _), materialized) = crate::clone_probe::measure(|| {
            store.query_window(&hot_request(None), 0, true).unwrap()
        });
        assert_eq!(all.len(), HISTORY);
        assert_eq!(materialized, HISTORY as u64);
    }

    #[test]
    fn query_window_cost_does_not_grow_with_entity_history() {
        // The same bounded page against a 10x longer history. Materialization
        // is bounded by the window, so history length must not show up in the
        // clone count — only the O(N) index scan that `total` inherently needs
        // still scales, and that clones no events.
        let short = seed_hot_entity(40);
        let long = seed_hot_entity(400);

        let (_, short_cost) = crate::clone_probe::measure(|| {
            short.query_window(&hot_request(Some(1)), 0, true).unwrap()
        });
        let (_, long_cost) = crate::clone_probe::measure(|| {
            long.query_window(&hot_request(Some(1)), 0, true).unwrap()
        });

        assert_eq!(
            (short_cost, long_cost),
            (1, 1),
            "a limit=1 page materialized {short_cost} events over a 40-event \
             history and {long_cost} over 400 — cost is tracking history length"
        );
    }

    #[test]
    fn select_window_orders_only_the_window() {
        // The complexity claim in `select_window`'s doc comment, made
        // observable: counting comparisons. A full sort of N items costs
        // ~N·log2(N); partial selection of a small window costs ~N. Replace the
        // `select_nth_unstable_by` with a plain `sort_unstable_by` over all N
        // and the ratio collapses, failing this test.
        use std::cell::Cell;
        const N: usize = 4096;

        let comparisons = Cell::new(0usize);
        let order = |a: &u64, b: &u64| {
            comparisons.set(comparisons.get() + 1);
            a.cmp(b)
        };
        let shuffled = || -> Vec<u64> {
            (0..N as u64)
                .map(|i| (i * 2_654_435_761) % 1_000_003)
                .collect()
        };

        let mut bounded = shuffled();
        select_window(&mut bounded, 0, Some(1), order);
        let bounded_cost = comparisons.replace(0);
        assert_eq!(bounded.len(), 1, "window of 1 keeps 1 item");

        let mut everything = shuffled();
        select_window(&mut everything, 0, None, order);
        let full_sort_cost = comparisons.get();
        assert_eq!(everything.len(), N);

        assert!(
            bounded_cost < 4 * N,
            "selecting a 1-item window out of {N} took {bounded_cost} comparisons \
             (~{}·N) — that is sort-shaped, not selection-shaped",
            bounded_cost / N
        );
        assert!(
            bounded_cost * 3 < full_sort_cost,
            "a 1-item window cost {bounded_cost} comparisons against \
             {full_sort_cost} for sorting all {N}: the window is not bounding \
             the ordering work"
        );
    }

    #[test]
    fn select_window_is_equivalent_to_sort_then_window() {
        // Exhaustive small-input check that partial selection cannot diverge
        // from sort-then-window, including duplicate keys (the total order is
        // supplied by the caller — here (key, position)).
        let order = |a: &(u32, usize), b: &(u32, usize)| a.cmp(b);
        let source: Vec<(u32, usize)> = [7, 3, 3, 9, 1, 3, 5, 9, 0, 2]
            .into_iter()
            .enumerate()
            .map(|(i, k)| (k, i))
            .collect();

        let mut sorted = source.clone();
        sorted.sort_by(order);

        for offset in 0..12 {
            for limit in [None, Some(0), Some(1), Some(3), Some(10), Some(50)] {
                let mut got = source.clone();
                select_window(&mut got, offset, limit, order);
                let got: Vec<_> = got.into_iter().skip(offset).collect();
                let expected: Vec<_> = sorted
                    .iter()
                    .copied()
                    .skip(offset)
                    .take(limit.unwrap_or(usize::MAX))
                    .collect();
                assert_eq!(got, expected, "offset={offset} limit={limit:?}");
            }
        }
    }

    // `as_of`/`since`/`until` are applied in `filter_entries`, which only runs
    // on the three INDEXED branches (entity_id / event_type / event_type_prefix).
    // The full-scan branch — a query scoped only by tenant, or only by
    // `payload_filter`/`exclude_event_type_prefix`, which is exactly what the
    // gateway sends for "this tenant's activity since T" — builds its offsets
    // as `(0..events.len())` and never calls it, and `apply_filters` did not
    // check timestamps either. So the time window was silently dropped: the
    // query returned the whole history, and `total`/`has_more` described that
    // whole history too. Same class of "the parameter never reaches the code
    // that would honour it" as #250's offset.
    #[test]
    fn query_window_applies_time_filters_on_the_full_scan_path() {
        let store = EventStore::new();
        let base = Utc::now() - chrono::Duration::hours(24);
        let mut ids = Vec::new();
        for i in 0..5i64 {
            let mut event = create_test_event(&format!("e-{i}"), "user.created");
            event.timestamp = base + chrono::Duration::hours(i);
            event.version = i + 1;
            ids.push(event.id);
            store.ingest(&event).unwrap();
        }
        let at = |h: i64| base + chrono::Duration::hours(h);

        // Tenant-only: no entity_id, no event_type, no event_type_prefix, so
        // nothing narrows the scan.
        let scoped = |mutate: &dyn Fn(&mut QueryEventsRequest)| {
            let mut req = QueryEventsRequest {
                tenant_id: Some("default".to_string()),
                ..QueryEventsRequest::default()
            };
            mutate(&mut req);
            req
        };

        for (label, req, expected) in [
            (
                "since=T+2 keeps only events at or after T+2",
                scoped(&|r| r.since = Some(at(2))),
                vec![ids[2], ids[3], ids[4]],
            ),
            (
                "until=T+1 keeps only events at or before T+1",
                scoped(&|r| r.until = Some(at(1))),
                vec![ids[0], ids[1]],
            ),
            (
                "as_of=T+1 is time travel: nothing newer than T+1",
                scoped(&|r| r.as_of = Some(at(1))),
                vec![ids[0], ids[1]],
            ),
            (
                "since+until compose into a closed window",
                scoped(&|r| {
                    r.since = Some(at(1));
                    r.until = Some(at(3));
                }),
                vec![ids[1], ids[2], ids[3]],
            ),
        ] {
            let (events, total) = store.query_window(&req, 0, false).unwrap();
            let got: Vec<_> = events.iter().map(|e| e.id).collect();
            assert_eq!(got, expected, "{label}");
            assert_eq!(
                total,
                expected.len(),
                "{label}: total counts the events INSIDE the window — \
                 has_more is derived from it, so a full-history total makes a \
                 paginator walk events the caller filtered out"
            );
        }

        // The indexed branches must keep agreeing with the full scan — the same
        // window asked for with an entity filter is the same window.
        let (indexed, total) = store
            .query_window(
                &scoped(&|r| {
                    r.entity_id = Some("e-3".to_string());
                    r.since = Some(at(2));
                }),
                0,
                false,
            )
            .unwrap();
        assert_eq!(
            indexed.iter().map(|e| e.id).collect::<Vec<_>>(),
            vec![ids[3]]
        );
        assert_eq!(total, 1);

        // And the window composes with the #251 selection: a bounded page taken
        // out of a time window is a page of that window, not of the history.
        let (page, total) = store
            .query_window(&scoped(&|r| r.since = Some(at(2))), 1, true)
            .unwrap();
        assert_eq!(
            page.iter().map(|e| e.id).collect::<Vec<_>>(),
            vec![ids[3], ids[2]],
            "order=desc + offset=1 inside a since window"
        );
        assert_eq!(total, 3);
    }

    #[test]
    fn query_window_bounded_selection_matches_a_full_sort() {
        // Bounded selection (partial select + sort of the window only) must be
        // observationally identical to sort-everything-then-window, including
        // ties and the descending path — otherwise the perf win silently
        // changes pagination.
        let store = EventStore::new();
        for i in 0..50 {
            store
                .ingest(&create_test_event(&format!("e-{i:02}"), "user.created"))
                .unwrap();
        }
        let all = |descending: bool| {
            let (events, _) = store
                .query_window(&QueryEventsRequest::default(), 0, descending)
                .unwrap();
            events
        };

        for descending in [false, true] {
            let reference = all(descending);
            for offset in [0, 1, 7, 49, 50, 100] {
                for limit in [1, 3, 10, 50, 100] {
                    let (page, total) = store
                        .query_window(
                            &QueryEventsRequest {
                                limit: Some(limit),
                                ..QueryEventsRequest::default()
                            },
                            offset,
                            descending,
                        )
                        .unwrap();
                    let expected: Vec<_> = reference
                        .iter()
                        .skip(offset)
                        .take(limit)
                        .map(|e| e.id)
                        .collect();
                    let got: Vec<_> = page.iter().map(|e| e.id).collect();
                    assert_eq!(
                        got, expected,
                        "desc={descending} offset={offset} limit={limit}: windowed \
                         selection must match a full sort"
                    );
                    assert_eq!(total, 50, "total is always the full match count");
                }
            }
        }
    }
}
