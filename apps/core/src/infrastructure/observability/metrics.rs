use prometheus::{
    Histogram, HistogramOpts, HistogramVec, IntCounter, IntCounterVec, IntGauge, IntGaugeVec, Opts,
    Registry,
};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

/// Centralized metrics registry for AllSource
pub struct MetricsRegistry {
    /// Prometheus registry
    registry: Registry,

    // Event ingestion metrics
    pub events_ingested_total: IntCounter,
    pub events_ingested_by_type: IntCounterVec,
    pub ingestion_duration_seconds: Histogram,
    pub ingestion_errors_total: IntCounter,

    // Query metrics
    pub queries_total: IntCounterVec,
    pub query_duration_seconds: HistogramVec,
    pub query_results_total: IntCounterVec,

    // Storage metrics
    pub storage_events_total: IntGauge,
    pub storage_entities_total: IntGauge,
    pub storage_size_bytes: IntGauge,
    pub parquet_files_total: IntGauge,
    pub wal_segments_total: IntGauge,

    // Projection metrics
    pub projections_total: IntGauge,
    pub projection_events_processed: IntCounterVec,
    pub projection_errors_total: IntCounterVec,
    pub projection_processing_duration: HistogramVec,
    pub projection_duration_seconds: Histogram,

    // Schema registry metrics (v0.5)
    pub schemas_registered_total: IntCounter,
    pub schema_validations_total: IntCounterVec,
    pub schema_validation_duration: Histogram,

    // Replay metrics (v0.5)
    pub replays_started_total: IntCounter,
    pub replays_completed_total: IntCounter,
    pub replays_failed_total: IntCounter,
    pub replay_events_processed: IntCounter,
    pub replay_duration_seconds: Histogram,

    // Pipeline metrics (v0.5)
    pub pipelines_registered_total: IntGauge,
    pub pipeline_events_processed: IntCounterVec,
    pub pipeline_events_filtered: IntCounterVec,
    pub pipeline_errors_total: IntCounterVec,
    pub pipeline_processing_duration: HistogramVec,
    pub pipeline_duration_seconds: Histogram,

    // Snapshot metrics
    pub snapshots_created_total: IntCounter,
    pub snapshot_creation_duration: Histogram,
    pub snapshots_total: IntGauge,

    // Compaction metrics
    pub compactions_total: IntCounter,
    pub compaction_duration_seconds: Histogram,
    pub compaction_files_merged: IntCounter,
    pub compaction_bytes_saved: IntCounter,

    // WebSocket metrics
    pub websocket_connections_active: IntGauge,
    pub websocket_connections_total: IntCounter,
    pub websocket_messages_sent: IntCounter,
    pub websocket_errors_total: IntCounter,

    // System metrics
    pub http_requests_total: IntCounterVec,
    pub http_request_duration_seconds: HistogramVec,
    pub http_requests_in_flight: IntGauge,

    // Replication metrics (leader)
    pub replication_followers_connected: IntGauge,
    pub replication_wal_shipped_total: IntCounter,
    pub replication_wal_shipped_bytes_total: IntCounter,
    pub replication_follower_lag_seconds: IntGaugeVec,
    pub replication_acks_total: IntCounter,

    // Replication ACK wait metric (semi-sync/sync mode)
    pub replication_ack_wait_seconds: Histogram,

    // Replication metrics (follower)
    pub replication_wal_received_total: IntCounter,
    pub replication_wal_replayed_total: IntCounter,
    pub replication_lag_seconds: IntGauge,
    pub replication_connected: IntGauge,
    pub replication_reconnects_total: IntCounter,

    // Cache metrics (Step 3 of the sustainable data strategy).
    // The cache is the in-memory representation of lazy-loaded
    // tenant data; both metrics are scoped to that side of the
    // system. `parquet_files_total` and `storage_size_bytes`
    // already cover disk-side state.
    pub cache_evictions_total: IntCounter,
    pub cache_bytes: IntGauge,

    // WAL replay metric (Step 6 of the sustainable data strategy).
    // Set on each successful WAL recovery; reflects the size of the
    // last replay. After a checkpoint truncates the WAL, the next
    // recovery should report a value bounded by the checkpoint
    // interval — that's the metric to graph to verify Step 6's
    // bounded-replay invariant.
    pub wal_replay_events_total: IntGauge,
}

impl MetricsRegistry {
    pub fn new() -> Arc<Self> {
        let registry = Registry::new();

        // Event ingestion metrics
        let events_ingested_total = IntCounter::with_opts(Opts::new(
            "allsource_events_ingested_total",
            "Total number of events ingested",
        ))
        .unwrap();

        let events_ingested_by_type = IntCounterVec::new(
            Opts::new(
                "allsource_events_ingested_by_type",
                "Events ingested by type",
            ),
            &["event_type"],
        )
        .unwrap();

        let ingestion_duration_seconds = Histogram::with_opts(HistogramOpts::new(
            "allsource_ingestion_duration_seconds",
            "Event ingestion duration in seconds",
        ))
        .unwrap();

        let ingestion_errors_total = IntCounter::with_opts(Opts::new(
            "allsource_ingestion_errors_total",
            "Total number of ingestion errors",
        ))
        .unwrap();

        // Query metrics
        let queries_total = IntCounterVec::new(
            Opts::new("allsource_queries_total", "Total number of queries"),
            &["query_type"],
        )
        .unwrap();

        let query_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "allsource_query_duration_seconds",
                "Query duration in seconds",
            ),
            &["query_type"],
        )
        .unwrap();

        let query_results_total = IntCounterVec::new(
            Opts::new(
                "allsource_query_results_total",
                "Total number of events returned by queries",
            ),
            &["query_type"],
        )
        .unwrap();

        // Storage metrics
        let storage_events_total = IntGauge::with_opts(Opts::new(
            "allsource_storage_events_total",
            "Total number of events in storage",
        ))
        .unwrap();

        let storage_entities_total = IntGauge::with_opts(Opts::new(
            "allsource_storage_entities_total",
            "Total number of entities in storage",
        ))
        .unwrap();

        let storage_size_bytes = IntGauge::with_opts(Opts::new(
            "allsource_storage_size_bytes",
            "Total storage size in bytes",
        ))
        .unwrap();

        let parquet_files_total = IntGauge::with_opts(Opts::new(
            "allsource_parquet_files_total",
            "Number of Parquet files",
        ))
        .unwrap();

        let wal_segments_total = IntGauge::with_opts(Opts::new(
            "allsource_wal_segments_total",
            "Number of WAL segments",
        ))
        .unwrap();

        // Projection metrics
        let projection_events_processed = IntCounterVec::new(
            Opts::new(
                "allsource_projection_events_processed",
                "Events processed by projections",
            ),
            &["projection_name"],
        )
        .unwrap();

        let projection_errors_total = IntCounterVec::new(
            Opts::new(
                "allsource_projection_errors_total",
                "Total projection errors",
            ),
            &["projection_name"],
        )
        .unwrap();

        let projection_processing_duration = HistogramVec::new(
            HistogramOpts::new(
                "allsource_projection_processing_duration_seconds",
                "Projection processing duration",
            ),
            &["projection_name"],
        )
        .unwrap();

        let projections_total = IntGauge::with_opts(Opts::new(
            "allsource_projections_total",
            "Number of registered projections",
        ))
        .unwrap();

        let projection_duration_seconds = Histogram::with_opts(HistogramOpts::new(
            "allsource_projection_duration_seconds",
            "Overall projection manager processing duration",
        ))
        .unwrap();

        // Schema registry metrics (v0.5)
        let schemas_registered_total = IntCounter::with_opts(Opts::new(
            "allsource_schemas_registered_total",
            "Total number of schemas registered",
        ))
        .unwrap();

        let schema_validations_total = IntCounterVec::new(
            Opts::new(
                "allsource_schema_validations_total",
                "Schema validations by result",
            ),
            &["subject", "result"],
        )
        .unwrap();

        let schema_validation_duration = Histogram::with_opts(HistogramOpts::new(
            "allsource_schema_validation_duration_seconds",
            "Schema validation duration",
        ))
        .unwrap();

        // Replay metrics (v0.5)
        let replays_started_total = IntCounter::with_opts(Opts::new(
            "allsource_replays_started_total",
            "Total replays started",
        ))
        .unwrap();

        let replays_completed_total = IntCounter::with_opts(Opts::new(
            "allsource_replays_completed_total",
            "Total replays completed",
        ))
        .unwrap();

        let replays_failed_total = IntCounter::with_opts(Opts::new(
            "allsource_replays_failed_total",
            "Total replays failed",
        ))
        .unwrap();

        let replay_events_processed = IntCounter::with_opts(Opts::new(
            "allsource_replay_events_processed",
            "Events processed during replays",
        ))
        .unwrap();

        let replay_duration_seconds = Histogram::with_opts(HistogramOpts::new(
            "allsource_replay_duration_seconds",
            "Replay duration",
        ))
        .unwrap();

        // Pipeline metrics (v0.5)
        let pipelines_registered_total = IntGauge::with_opts(Opts::new(
            "allsource_pipelines_registered_total",
            "Number of registered pipelines",
        ))
        .unwrap();

        let pipeline_events_processed = IntCounterVec::new(
            Opts::new(
                "allsource_pipeline_events_processed",
                "Events processed by pipelines",
            ),
            &["pipeline_id", "pipeline_name"],
        )
        .unwrap();

        let pipeline_events_filtered = IntCounterVec::new(
            Opts::new(
                "allsource_pipeline_events_filtered",
                "Events filtered by pipelines",
            ),
            &["pipeline_id", "pipeline_name"],
        )
        .unwrap();

        let pipeline_processing_duration = HistogramVec::new(
            HistogramOpts::new(
                "allsource_pipeline_processing_duration_seconds",
                "Pipeline processing duration",
            ),
            &["pipeline_id", "pipeline_name"],
        )
        .unwrap();

        let pipeline_errors_total = IntCounterVec::new(
            Opts::new("allsource_pipeline_errors_total", "Total pipeline errors"),
            &["pipeline_name"],
        )
        .unwrap();

        let pipeline_duration_seconds = Histogram::with_opts(HistogramOpts::new(
            "allsource_pipeline_duration_seconds",
            "Overall pipeline manager processing duration",
        ))
        .unwrap();

        // Snapshot metrics
        let snapshots_created_total = IntCounter::with_opts(Opts::new(
            "allsource_snapshots_created_total",
            "Total snapshots created",
        ))
        .unwrap();

        let snapshot_creation_duration = Histogram::with_opts(HistogramOpts::new(
            "allsource_snapshot_creation_duration_seconds",
            "Snapshot creation duration",
        ))
        .unwrap();

        let snapshots_total = IntGauge::with_opts(Opts::new(
            "allsource_snapshots_total",
            "Total number of snapshots",
        ))
        .unwrap();

        // Compaction metrics
        let compactions_total = IntCounter::with_opts(Opts::new(
            "allsource_compactions_total",
            "Total compactions performed",
        ))
        .unwrap();

        let compaction_duration_seconds = Histogram::with_opts(HistogramOpts::new(
            "allsource_compaction_duration_seconds",
            "Compaction duration",
        ))
        .unwrap();

        let compaction_files_merged = IntCounter::with_opts(Opts::new(
            "allsource_compaction_files_merged",
            "Files merged during compaction",
        ))
        .unwrap();

        let compaction_bytes_saved = IntCounter::with_opts(Opts::new(
            "allsource_compaction_bytes_saved",
            "Bytes saved by compaction",
        ))
        .unwrap();

        // Cache metrics (Step 3): per-tenant in-memory eviction
        let cache_evictions_total = IntCounter::with_opts(Opts::new(
            "allsource_core_cache_evictions_total",
            "Total tenant evictions from the in-memory cache",
        ))
        .unwrap();

        let cache_bytes = IntGauge::with_opts(Opts::new(
            "allsource_core_cache_bytes",
            "Approximate resident bytes in the in-memory tenant cache",
        ))
        .unwrap();

        // WAL replay metric (Step 6): size of last recovery.
        let wal_replay_events_total = IntGauge::with_opts(Opts::new(
            "allsource_core_wal_replay_events_total",
            "Number of events replayed from the WAL on the most recent boot",
        ))
        .unwrap();

        // WebSocket metrics
        let websocket_connections_active = IntGauge::with_opts(Opts::new(
            "allsource_websocket_connections_active",
            "Active WebSocket connections",
        ))
        .unwrap();

        let websocket_connections_total = IntCounter::with_opts(Opts::new(
            "allsource_websocket_connections_total",
            "Total WebSocket connections",
        ))
        .unwrap();

        let websocket_messages_sent = IntCounter::with_opts(Opts::new(
            "allsource_websocket_messages_sent",
            "WebSocket messages sent",
        ))
        .unwrap();

        let websocket_errors_total = IntCounter::with_opts(Opts::new(
            "allsource_websocket_errors_total",
            "WebSocket errors",
        ))
        .unwrap();

        // System metrics
        let http_requests_total = IntCounterVec::new(
            Opts::new("allsource_http_requests_total", "Total HTTP requests"),
            &["method", "endpoint", "status"],
        )
        .unwrap();

        let http_request_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "allsource_http_request_duration_seconds",
                "HTTP request duration",
            ),
            &["method", "endpoint"],
        )
        .unwrap();

        let http_requests_in_flight = IntGauge::with_opts(Opts::new(
            "allsource_http_requests_in_flight",
            "HTTP requests currently being processed",
        ))
        .unwrap();

        // Replication metrics (leader)
        let replication_followers_connected = IntGauge::with_opts(Opts::new(
            "allsource_replication_followers_connected",
            "Number of connected followers",
        ))
        .unwrap();

        let replication_wal_shipped_total = IntCounter::with_opts(Opts::new(
            "allsource_replication_wal_shipped_total",
            "Total WAL entries shipped to followers",
        ))
        .unwrap();

        let replication_wal_shipped_bytes_total = IntCounter::with_opts(Opts::new(
            "allsource_replication_wal_shipped_bytes_total",
            "Total bytes shipped to followers",
        ))
        .unwrap();

        let replication_follower_lag_seconds = IntGaugeVec::new(
            Opts::new(
                "allsource_replication_follower_lag_seconds",
                "Per-follower replication lag in seconds",
            ),
            &["follower_id"],
        )
        .unwrap();

        let replication_acks_total = IntCounter::with_opts(Opts::new(
            "allsource_replication_acks_total",
            "Total ACKs received from followers",
        ))
        .unwrap();

        let replication_ack_wait_seconds = Histogram::with_opts(
            HistogramOpts::new(
                "allsource_replication_ack_wait_seconds",
                "Time spent waiting for follower ACKs in semi-sync/sync mode",
            )
            .buckets(vec![
                0.001, 0.002, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0,
            ]),
        )
        .unwrap();

        // Replication metrics (follower)
        let replication_wal_received_total = IntCounter::with_opts(Opts::new(
            "allsource_replication_wal_received_total",
            "Total WAL entries received from leader",
        ))
        .unwrap();

        let replication_wal_replayed_total = IntCounter::with_opts(Opts::new(
            "allsource_replication_wal_replayed_total",
            "Total WAL entries replayed into DashMap",
        ))
        .unwrap();

        let replication_lag_seconds = IntGauge::with_opts(Opts::new(
            "allsource_replication_lag_seconds",
            "Replication lag behind leader in seconds",
        ))
        .unwrap();

        let replication_connected = IntGauge::with_opts(Opts::new(
            "allsource_replication_connected",
            "Whether connected to leader (1=connected, 0=disconnected)",
        ))
        .unwrap();

        let replication_reconnects_total = IntCounter::with_opts(Opts::new(
            "allsource_replication_reconnects_total",
            "Total reconnection attempts to leader",
        ))
        .unwrap();

        // Register all metrics
        registry
            .register(Box::new(events_ingested_total.clone()))
            .unwrap();
        registry
            .register(Box::new(events_ingested_by_type.clone()))
            .unwrap();
        registry
            .register(Box::new(ingestion_duration_seconds.clone()))
            .unwrap();
        registry
            .register(Box::new(ingestion_errors_total.clone()))
            .unwrap();

        registry.register(Box::new(queries_total.clone())).unwrap();
        registry
            .register(Box::new(query_duration_seconds.clone()))
            .unwrap();
        registry
            .register(Box::new(query_results_total.clone()))
            .unwrap();

        registry
            .register(Box::new(storage_events_total.clone()))
            .unwrap();
        registry
            .register(Box::new(storage_entities_total.clone()))
            .unwrap();
        registry
            .register(Box::new(storage_size_bytes.clone()))
            .unwrap();
        registry
            .register(Box::new(parquet_files_total.clone()))
            .unwrap();
        registry
            .register(Box::new(wal_segments_total.clone()))
            .unwrap();

        registry
            .register(Box::new(projection_events_processed.clone()))
            .unwrap();
        registry
            .register(Box::new(projection_errors_total.clone()))
            .unwrap();
        registry
            .register(Box::new(projection_processing_duration.clone()))
            .unwrap();
        registry
            .register(Box::new(projections_total.clone()))
            .unwrap();
        registry
            .register(Box::new(projection_duration_seconds.clone()))
            .unwrap();

        registry
            .register(Box::new(schemas_registered_total.clone()))
            .unwrap();
        registry
            .register(Box::new(schema_validations_total.clone()))
            .unwrap();
        registry
            .register(Box::new(schema_validation_duration.clone()))
            .unwrap();

        registry
            .register(Box::new(replays_started_total.clone()))
            .unwrap();
        registry
            .register(Box::new(replays_completed_total.clone()))
            .unwrap();
        registry
            .register(Box::new(replays_failed_total.clone()))
            .unwrap();
        registry
            .register(Box::new(replay_events_processed.clone()))
            .unwrap();
        registry
            .register(Box::new(replay_duration_seconds.clone()))
            .unwrap();

        registry
            .register(Box::new(pipelines_registered_total.clone()))
            .unwrap();
        registry
            .register(Box::new(pipeline_events_processed.clone()))
            .unwrap();
        registry
            .register(Box::new(pipeline_events_filtered.clone()))
            .unwrap();
        registry
            .register(Box::new(pipeline_processing_duration.clone()))
            .unwrap();
        registry
            .register(Box::new(pipeline_errors_total.clone()))
            .unwrap();
        registry
            .register(Box::new(pipeline_duration_seconds.clone()))
            .unwrap();

        registry
            .register(Box::new(snapshots_created_total.clone()))
            .unwrap();
        registry
            .register(Box::new(snapshot_creation_duration.clone()))
            .unwrap();
        registry
            .register(Box::new(snapshots_total.clone()))
            .unwrap();

        registry
            .register(Box::new(compactions_total.clone()))
            .unwrap();
        registry
            .register(Box::new(compaction_duration_seconds.clone()))
            .unwrap();
        registry
            .register(Box::new(compaction_files_merged.clone()))
            .unwrap();
        registry
            .register(Box::new(compaction_bytes_saved.clone()))
            .unwrap();

        registry
            .register(Box::new(cache_evictions_total.clone()))
            .unwrap();
        registry.register(Box::new(cache_bytes.clone())).unwrap();
        registry
            .register(Box::new(wal_replay_events_total.clone()))
            .unwrap();

        registry
            .register(Box::new(websocket_connections_active.clone()))
            .unwrap();
        registry
            .register(Box::new(websocket_connections_total.clone()))
            .unwrap();
        registry
            .register(Box::new(websocket_messages_sent.clone()))
            .unwrap();
        registry
            .register(Box::new(websocket_errors_total.clone()))
            .unwrap();

        registry
            .register(Box::new(http_requests_total.clone()))
            .unwrap();
        registry
            .register(Box::new(http_request_duration_seconds.clone()))
            .unwrap();
        registry
            .register(Box::new(http_requests_in_flight.clone()))
            .unwrap();

        registry
            .register(Box::new(replication_followers_connected.clone()))
            .unwrap();
        registry
            .register(Box::new(replication_wal_shipped_total.clone()))
            .unwrap();
        registry
            .register(Box::new(replication_wal_shipped_bytes_total.clone()))
            .unwrap();
        registry
            .register(Box::new(replication_follower_lag_seconds.clone()))
            .unwrap();
        registry
            .register(Box::new(replication_acks_total.clone()))
            .unwrap();
        registry
            .register(Box::new(replication_ack_wait_seconds.clone()))
            .unwrap();
        registry
            .register(Box::new(replication_wal_received_total.clone()))
            .unwrap();
        registry
            .register(Box::new(replication_wal_replayed_total.clone()))
            .unwrap();
        registry
            .register(Box::new(replication_lag_seconds.clone()))
            .unwrap();
        registry
            .register(Box::new(replication_connected.clone()))
            .unwrap();
        registry
            .register(Box::new(replication_reconnects_total.clone()))
            .unwrap();

        Arc::new(Self {
            registry,
            events_ingested_total,
            events_ingested_by_type,
            ingestion_duration_seconds,
            ingestion_errors_total,
            queries_total,
            query_duration_seconds,
            query_results_total,
            storage_events_total,
            storage_entities_total,
            storage_size_bytes,
            parquet_files_total,
            wal_segments_total,
            projections_total,
            projection_events_processed,
            projection_errors_total,
            projection_processing_duration,
            projection_duration_seconds,
            schemas_registered_total,
            schema_validations_total,
            schema_validation_duration,
            replays_started_total,
            replays_completed_total,
            replays_failed_total,
            replay_events_processed,
            replay_duration_seconds,
            pipelines_registered_total,
            pipeline_events_processed,
            pipeline_events_filtered,
            pipeline_errors_total,
            pipeline_processing_duration,
            pipeline_duration_seconds,
            snapshots_created_total,
            snapshot_creation_duration,
            snapshots_total,
            compactions_total,
            compaction_duration_seconds,
            compaction_files_merged,
            compaction_bytes_saved,
            websocket_connections_active,
            websocket_connections_total,
            websocket_messages_sent,
            websocket_errors_total,
            http_requests_total,
            http_request_duration_seconds,
            http_requests_in_flight,
            replication_followers_connected,
            replication_wal_shipped_total,
            replication_wal_shipped_bytes_total,
            replication_follower_lag_seconds,
            replication_acks_total,
            replication_ack_wait_seconds,
            replication_wal_received_total,
            replication_wal_replayed_total,
            replication_lag_seconds,
            replication_connected,
            replication_reconnects_total,
            cache_evictions_total,
            cache_bytes,
            wal_replay_events_total,
        })
    }

    /// Get the Prometheus registry
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Record one completed compaction sweep.
    ///
    /// Takes scalars rather than a `CompactionResult` so observability keeps
    /// no dependency on the persistence layer.
    pub fn record_compaction_sweep(
        &self,
        files_compacted: u64,
        bytes_before: u64,
        bytes_after: u64,
    ) {
        self.compaction_files_merged.inc_by(files_compacted);
        self.compaction_bytes_saved
            .inc_by(bytes_before.saturating_sub(bytes_after));
    }

    /// Encode metrics in Prometheus text format
    pub fn encode(&self) -> Result<String, Box<dyn std::error::Error>> {
        use prometheus::Encoder;
        let encoder = prometheus::TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer)?;
        Ok(String::from_utf8(buffer)?)
    }
}

// Note: Clone and Default are intentionally NOT implemented for MetricsRegistry.
// Use Arc<MetricsRegistry> to share the same registry across the application.
// Creating multiple registries would result in duplicate metrics which is incorrect.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_registry_creation() {
        let metrics = MetricsRegistry::new();
        assert_eq!(metrics.events_ingested_total.get(), 0);
        assert_eq!(metrics.storage_events_total.get(), 0);
    }

    #[test]
    fn test_event_ingestion_metrics() {
        let metrics = MetricsRegistry::new();

        // Increment ingestion counter
        metrics.events_ingested_total.inc();
        assert_eq!(metrics.events_ingested_total.get(), 1);

        // Increment by type
        metrics
            .events_ingested_by_type
            .with_label_values(&["user.created"])
            .inc();
        assert_eq!(
            metrics
                .events_ingested_by_type
                .with_label_values(&["user.created"])
                .get(),
            1
        );

        // Record duration
        metrics.ingestion_duration_seconds.observe(0.1);
    }

    #[test]
    fn test_query_metrics() {
        let metrics = MetricsRegistry::new();

        // Increment query counter
        metrics
            .queries_total
            .with_label_values(&["entity_id"])
            .inc();
        assert_eq!(
            metrics
                .queries_total
                .with_label_values(&["entity_id"])
                .get(),
            1
        );

        // Record query duration
        metrics
            .query_duration_seconds
            .with_label_values(&["entity_id"])
            .observe(0.05);

        // Record query results
        metrics
            .query_results_total
            .with_label_values(&["entity_id"])
            .inc_by(10);
    }

    #[test]
    fn test_storage_metrics() {
        let metrics = MetricsRegistry::new();

        // Set storage metrics
        metrics.storage_events_total.set(1000);
        assert_eq!(metrics.storage_events_total.get(), 1000);

        metrics.storage_entities_total.set(50);
        assert_eq!(metrics.storage_entities_total.get(), 50);

        metrics.storage_size_bytes.set(1024 * 1024);
        assert_eq!(metrics.storage_size_bytes.get(), 1024 * 1024);

        metrics.parquet_files_total.set(5);
        metrics.wal_segments_total.set(3);
    }

    #[test]
    fn test_projection_metrics() {
        let metrics = MetricsRegistry::new();

        // Set projections total
        metrics.projections_total.set(3);
        assert_eq!(metrics.projections_total.get(), 3);

        // Process events in projection
        metrics
            .projection_events_processed
            .with_label_values(&["user_snapshot"])
            .inc_by(100);

        // Record processing duration
        metrics
            .projection_processing_duration
            .with_label_values(&["user_snapshot"])
            .observe(0.2);

        // Record errors
        metrics
            .projection_errors_total
            .with_label_values(&["user_snapshot"])
            .inc();
    }

    #[test]
    fn test_schema_metrics() {
        let metrics = MetricsRegistry::new();

        // Register schema
        metrics.schemas_registered_total.inc();
        assert_eq!(metrics.schemas_registered_total.get(), 1);

        // Validation success - requires both subject and result labels
        metrics
            .schema_validations_total
            .with_label_values(&["user.schema", "success"])
            .inc();

        // Validation failure
        metrics
            .schema_validations_total
            .with_label_values(&["order.schema", "failure"])
            .inc();

        // Record validation duration
        metrics.schema_validation_duration.observe(0.01);
    }

    #[test]
    fn test_replay_metrics() {
        let metrics = MetricsRegistry::new();

        // Start replay
        metrics.replays_started_total.inc();
        assert_eq!(metrics.replays_started_total.get(), 1);

        // Process events
        metrics.replay_events_processed.inc_by(500);
        assert_eq!(metrics.replay_events_processed.get(), 500);

        // Complete replay
        metrics.replays_completed_total.inc();
        assert_eq!(metrics.replays_completed_total.get(), 1);

        // Record duration
        metrics.replay_duration_seconds.observe(5.5);
    }

    #[test]
    fn test_pipeline_metrics() {
        let metrics = MetricsRegistry::new();

        // Register pipeline
        metrics.pipelines_registered_total.set(2);
        assert_eq!(metrics.pipelines_registered_total.get(), 2);

        // Process events - requires both pipeline_id and pipeline_name labels
        metrics
            .pipeline_events_processed
            .with_label_values(&["pipeline-1", "filter_pipeline"])
            .inc_by(250);

        // Record errors - only requires pipeline_name
        metrics
            .pipeline_errors_total
            .with_label_values(&["filter_pipeline"])
            .inc();

        // Record duration - requires both pipeline_id and pipeline_name labels
        metrics
            .pipeline_processing_duration
            .with_label_values(&["pipeline-1", "filter_pipeline"])
            .observe(0.15);
    }

    #[test]
    fn test_metrics_encode() {
        let metrics = MetricsRegistry::new();

        // Add some data
        metrics.events_ingested_total.inc_by(100);
        metrics.storage_events_total.set(1000);

        // Encode to Prometheus format
        let encoded = metrics.encode().unwrap();

        // Verify output contains metrics
        assert!(encoded.contains("events_ingested_total"));
        assert!(encoded.contains("storage_events_total"));
    }

    #[test]
    fn test_metrics_default() {
        let metrics = MetricsRegistry::new();
        assert_eq!(metrics.events_ingested_total.get(), 0);
    }

    #[test]
    fn test_websocket_metrics() {
        let metrics = MetricsRegistry::new();

        // Connect client
        metrics.websocket_connections_active.inc();
        assert_eq!(metrics.websocket_connections_active.get(), 1);

        // Total connections
        metrics.websocket_connections_total.inc();

        // Broadcast message
        metrics.websocket_messages_sent.inc_by(10);
        assert_eq!(metrics.websocket_messages_sent.get(), 10);

        // Disconnect client
        metrics.websocket_connections_active.dec();
        assert_eq!(metrics.websocket_connections_active.get(), 0);

        // Record error
        metrics.websocket_errors_total.inc();
    }

    #[test]
    fn test_compaction_metrics() {
        let metrics = MetricsRegistry::new();

        // Start compaction
        metrics.compactions_total.inc();
        assert_eq!(metrics.compactions_total.get(), 1);

        // Record duration
        metrics.compaction_duration_seconds.observe(5.2);

        // Files merged
        metrics.compaction_files_merged.inc_by(5);

        // Bytes saved
        metrics.compaction_bytes_saved.inc_by(1024 * 1024);
    }

    #[test]
    fn a_recorded_sweep_moves_both_compaction_counters() {
        let metrics = MetricsRegistry::new();

        metrics.record_compaction_sweep(92, 18_517_874, 18_222_899);

        assert_eq!(metrics.compaction_files_merged.get(), 92);
        assert_eq!(metrics.compaction_bytes_saved.get(), 294_975);
    }

    #[test]
    fn a_sweep_that_grew_the_archive_saves_no_bytes_rather_than_underflowing() {
        let metrics = MetricsRegistry::new();

        metrics.record_compaction_sweep(3, 1_000, 1_500);

        assert_eq!(metrics.compaction_files_merged.get(), 3);
        assert_eq!(metrics.compaction_bytes_saved.get(), 0);
    }

    #[test]
    fn compaction_counters_are_exported_under_their_documented_names() {
        let metrics = MetricsRegistry::new();

        metrics.record_compaction_sweep(7, 500, 100);
        let exported = metrics.encode().expect("encode metrics");

        assert!(
            exported.contains("allsource_compaction_files_merged 7"),
            "files_merged missing from /metrics output:\n{exported}"
        );
        assert!(
            exported.contains("allsource_compaction_bytes_saved 400"),
            "bytes_saved missing from /metrics output:\n{exported}"
        );
    }

    #[test]
    fn test_snapshot_metrics() {
        let metrics = MetricsRegistry::new();

        // Create snapshot
        metrics.snapshots_created_total.inc();
        assert_eq!(metrics.snapshots_created_total.get(), 1);

        // Record duration
        metrics.snapshot_creation_duration.observe(0.5);

        // Total snapshots
        metrics.snapshots_total.set(10);
        assert_eq!(metrics.snapshots_total.get(), 10);
    }

    #[test]
    fn test_replication_leader_metrics() {
        let metrics = MetricsRegistry::new();

        // Follower connected
        metrics.replication_followers_connected.set(2);
        assert_eq!(metrics.replication_followers_connected.get(), 2);

        // WAL entries shipped
        metrics.replication_wal_shipped_total.inc_by(100);
        assert_eq!(metrics.replication_wal_shipped_total.get(), 100);

        // Bytes shipped
        metrics
            .replication_wal_shipped_bytes_total
            .inc_by(1024 * 1024);
        assert_eq!(
            metrics.replication_wal_shipped_bytes_total.get(),
            1024 * 1024
        );

        // Per-follower lag
        metrics
            .replication_follower_lag_seconds
            .with_label_values(&["follower-1"])
            .set(5);
        assert_eq!(
            metrics
                .replication_follower_lag_seconds
                .with_label_values(&["follower-1"])
                .get(),
            5
        );

        // ACKs received
        metrics.replication_acks_total.inc_by(50);
        assert_eq!(metrics.replication_acks_total.get(), 50);
    }

    #[test]
    fn test_replication_follower_metrics() {
        let metrics = MetricsRegistry::new();

        // WAL entries received
        metrics.replication_wal_received_total.inc_by(200);
        assert_eq!(metrics.replication_wal_received_total.get(), 200);

        // WAL entries replayed
        metrics.replication_wal_replayed_total.inc_by(195);
        assert_eq!(metrics.replication_wal_replayed_total.get(), 195);

        // Lag behind leader
        metrics.replication_lag_seconds.set(3);
        assert_eq!(metrics.replication_lag_seconds.get(), 3);

        // Connected state
        metrics.replication_connected.set(1);
        assert_eq!(metrics.replication_connected.get(), 1);

        // Reconnects
        metrics.replication_reconnects_total.inc_by(2);
        assert_eq!(metrics.replication_reconnects_total.get(), 2);
    }

    #[test]
    fn test_replication_metrics_in_encode() {
        let metrics = MetricsRegistry::new();

        metrics.replication_followers_connected.set(1);
        metrics.replication_wal_shipped_total.inc();
        metrics.replication_connected.set(1);

        let encoded = metrics.encode().unwrap();
        assert!(encoded.contains("allsource_replication_followers_connected"));
        assert!(encoded.contains("allsource_replication_wal_shipped_total"));
        assert!(encoded.contains("allsource_replication_connected"));
    }

    #[test]
    fn test_http_metrics() {
        let metrics = MetricsRegistry::new();

        // Record request
        metrics
            .http_requests_total
            .with_label_values(&["GET", "/api/events", "200"])
            .inc();

        // Record duration
        metrics
            .http_request_duration_seconds
            .with_label_values(&["GET", "/api/events"])
            .observe(0.025);

        // In-flight requests
        metrics.http_requests_in_flight.inc();
        assert_eq!(metrics.http_requests_in_flight.get(), 1);

        metrics.http_requests_in_flight.dec();
        assert_eq!(metrics.http_requests_in_flight.get(), 0);
    }
}

// =============================================================================
// Partition Metrics (SierraDB pattern)
// =============================================================================

/// Per-partition statistics for detecting hot partitions and skew
///
/// SierraDB uses 32 fixed partitions for single-node, 1024+ for clusters.
/// This struct tracks metrics per partition to detect imbalances.
#[derive(Debug)]
pub struct PartitionStats {
    /// Partition ID (0 to partition_count-1)
    pub partition_id: u32,

    /// Total events written to this partition
    pub event_count: u64,

    /// Total write latency sum (nanoseconds) for calculating average
    pub total_latency_ns: u64,

    /// Number of writes for calculating average latency
    pub write_count: u64,

    /// Minimum write latency (nanoseconds)
    pub min_latency_ns: u64,

    /// Maximum write latency (nanoseconds)
    pub max_latency_ns: u64,

    /// Total error count for this partition
    pub error_count: u64,
}

impl PartitionStats {
    fn new(partition_id: u32) -> Self {
        Self {
            partition_id,
            event_count: 0,
            total_latency_ns: 0,
            write_count: 0,
            min_latency_ns: u64::MAX,
            max_latency_ns: 0,
            error_count: 0,
        }
    }

    /// Calculate average write latency
    pub fn avg_latency(&self) -> Option<Duration> {
        self.total_latency_ns
            .checked_div(self.write_count)
            .map(Duration::from_nanos)
    }
}

/// Alert generated when partition imbalance is detected
#[derive(Debug, Clone, Serialize)]
pub struct PartitionImbalanceAlert {
    /// Partition ID that is imbalanced
    pub partition_id: u32,

    /// Event count for this partition
    pub event_count: u64,

    /// Average event count across all partitions
    pub average_count: f64,

    /// Ratio compared to average (>2.0 triggers alert)
    pub ratio_to_average: f64,

    /// Alert message
    pub message: String,

    /// Timestamp when alert was generated
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Internal structure for tracking partition metrics atomically
struct PartitionMetricsEntry {
    event_count: AtomicU64,
    total_latency_ns: AtomicU64,
    write_count: AtomicU64,
    min_latency_ns: AtomicU64,
    max_latency_ns: AtomicU64,
    error_count: AtomicU64,
}

impl PartitionMetricsEntry {
    fn new() -> Self {
        Self {
            event_count: AtomicU64::new(0),
            total_latency_ns: AtomicU64::new(0),
            write_count: AtomicU64::new(0),
            min_latency_ns: AtomicU64::new(u64::MAX),
            max_latency_ns: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
        }
    }
}

/// Partition monitoring for detecting hot partitions and skew (SierraDB pattern)
///
/// # Design Pattern
/// Uses atomic operations for lock-free metric updates per partition.
/// Tracks event counts, write latencies, and error rates per partition.
///
/// # SierraDB Context
/// SierraDB uses fixed partitions (32 for single-node, 1024+ for clusters).
/// Detecting hot partitions is critical for:
/// - Load balancing decisions
/// - Identifying skewed hash functions
/// - Capacity planning
/// - Performance troubleshooting
///
/// # Imbalance Detection
/// A partition is considered imbalanced if it has >2x the average load.
/// This threshold is based on SierraDB's experience with production workloads.
///
/// # Example
/// ```ignore
/// let partition_metrics = PartitionMetrics::new(32);
///
/// // Record write to partition 5
/// let start = Instant::now();
/// // ... write operation ...
/// partition_metrics.record_write(5, start.elapsed());
///
/// // Check for imbalances
/// let alerts = partition_metrics.detect_partition_imbalance();
/// for alert in alerts {
///     tracing::warn!("Partition imbalance: {}", alert.message);
/// }
/// ```
pub struct PartitionMetrics {
    /// Number of partitions
    partition_count: u32,

    /// Per-partition metrics
    partitions: Vec<PartitionMetricsEntry>,

    /// Prometheus metrics for per-partition event counts
    partition_events_total: IntGaugeVec,

    /// Prometheus metrics for per-partition write latency histogram
    partition_write_latency: HistogramVec,

    /// Prometheus metrics for per-partition error counts
    partition_errors_total: IntCounterVec,

    /// Prometheus registry (for registration)
    registry: Registry,

    /// Timestamp when metrics collection started
    started_at: Instant,
}

impl PartitionMetrics {
    /// Create a new partition metrics tracker
    ///
    /// # Arguments
    /// * `partition_count` - Number of partitions (default: 32 for single-node)
    pub fn new(partition_count: u32) -> Self {
        let registry = Registry::new();

        // Per-partition event count gauge
        let partition_events_total = IntGaugeVec::new(
            Opts::new(
                "allsource_partition_events_total",
                "Total events per partition",
            ),
            &["partition_id"],
        )
        .expect("Failed to create partition_events_total metric");

        // Per-partition write latency histogram
        let partition_write_latency = HistogramVec::new(
            HistogramOpts::new(
                "allsource_partition_write_latency_seconds",
                "Write latency per partition in seconds",
            )
            .buckets(vec![
                0.0001, 0.0005, 0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0,
            ]),
            &["partition_id"],
        )
        .expect("Failed to create partition_write_latency metric");

        // Per-partition error counter
        let partition_errors_total = IntCounterVec::new(
            Opts::new(
                "allsource_partition_errors_total",
                "Total errors per partition",
            ),
            &["partition_id"],
        )
        .expect("Failed to create partition_errors_total metric");

        // Register metrics
        registry
            .register(Box::new(partition_events_total.clone()))
            .expect("Failed to register partition_events_total");
        registry
            .register(Box::new(partition_write_latency.clone()))
            .expect("Failed to register partition_write_latency");
        registry
            .register(Box::new(partition_errors_total.clone()))
            .expect("Failed to register partition_errors_total");

        // Initialize per-partition atomic counters
        let partitions = (0..partition_count)
            .map(|_| PartitionMetricsEntry::new())
            .collect();

        Self {
            partition_count,
            partitions,
            partition_events_total,
            partition_write_latency,
            partition_errors_total,
            registry,
            started_at: Instant::now(),
        }
    }

    /// Create partition metrics with default partition count (32)
    pub fn with_default_partitions() -> Self {
        Self::new(32)
    }

    /// Record a successful write to a partition
    ///
    /// # Arguments
    /// * `partition_id` - The partition ID (0 to partition_count-1)
    /// * `latency` - The write latency
    #[inline]
    pub fn record_write(&self, partition_id: u32, latency: Duration) {
        if partition_id >= self.partition_count {
            return;
        }

        let entry = &self.partitions[partition_id as usize];
        let latency_ns = latency.as_nanos() as u64;

        // Update atomic counters
        entry.event_count.fetch_add(1, Ordering::Relaxed);
        entry.write_count.fetch_add(1, Ordering::Relaxed);
        entry
            .total_latency_ns
            .fetch_add(latency_ns, Ordering::Relaxed);

        // Update min latency (compare-and-swap loop)
        let mut current_min = entry.min_latency_ns.load(Ordering::Relaxed);
        while latency_ns < current_min {
            match entry.min_latency_ns.compare_exchange_weak(
                current_min,
                latency_ns,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current_min = actual,
            }
        }

        // Update max latency (compare-and-swap loop)
        let mut current_max = entry.max_latency_ns.load(Ordering::Relaxed);
        while latency_ns > current_max {
            match entry.max_latency_ns.compare_exchange_weak(
                current_max,
                latency_ns,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current_max = actual,
            }
        }

        // Update Prometheus metrics
        let partition_id_str = partition_id.to_string();
        self.partition_events_total
            .with_label_values(&[&partition_id_str])
            .set(entry.event_count.load(Ordering::Relaxed) as i64);
        self.partition_write_latency
            .with_label_values(&[&partition_id_str])
            .observe(latency.as_secs_f64());
    }

    /// Record an error for a partition
    ///
    /// # Arguments
    /// * `partition_id` - The partition ID (0 to partition_count-1)
    #[inline]
    pub fn record_error(&self, partition_id: u32) {
        if partition_id >= self.partition_count {
            return;
        }

        let entry = &self.partitions[partition_id as usize];
        entry.error_count.fetch_add(1, Ordering::Relaxed);

        // Update Prometheus metrics
        let partition_id_str = partition_id.to_string();
        self.partition_errors_total
            .with_label_values(&[&partition_id_str])
            .inc();
    }

    /// Record a batch write to a partition
    ///
    /// # Arguments
    /// * `partition_id` - The partition ID (0 to partition_count-1)
    /// * `count` - Number of events in the batch
    /// * `latency` - Total latency for the batch write
    #[inline]
    pub fn record_batch_write(&self, partition_id: u32, count: u64, latency: Duration) {
        if partition_id >= self.partition_count {
            return;
        }

        let entry = &self.partitions[partition_id as usize];
        let latency_ns = latency.as_nanos() as u64;

        // Update atomic counters
        entry.event_count.fetch_add(count, Ordering::Relaxed);
        entry.write_count.fetch_add(1, Ordering::Relaxed);
        entry
            .total_latency_ns
            .fetch_add(latency_ns, Ordering::Relaxed);

        // Update min/max latency using per-event average
        let per_event_latency_ns = latency_ns / count.max(1);

        // Update min latency
        let mut current_min = entry.min_latency_ns.load(Ordering::Relaxed);
        while per_event_latency_ns < current_min {
            match entry.min_latency_ns.compare_exchange_weak(
                current_min,
                per_event_latency_ns,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current_min = actual,
            }
        }

        // Update max latency
        let mut current_max = entry.max_latency_ns.load(Ordering::Relaxed);
        while per_event_latency_ns > current_max {
            match entry.max_latency_ns.compare_exchange_weak(
                current_max,
                per_event_latency_ns,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current_max = actual,
            }
        }

        // Update Prometheus metrics
        let partition_id_str = partition_id.to_string();
        self.partition_events_total
            .with_label_values(&[&partition_id_str])
            .set(entry.event_count.load(Ordering::Relaxed) as i64);
        self.partition_write_latency
            .with_label_values(&[&partition_id_str])
            .observe(latency.as_secs_f64());
    }

    /// Get statistics for a specific partition
    pub fn get_partition_stats(&self, partition_id: u32) -> Option<PartitionStats> {
        if partition_id >= self.partition_count {
            return None;
        }

        let entry = &self.partitions[partition_id as usize];

        Some(PartitionStats {
            partition_id,
            event_count: entry.event_count.load(Ordering::Relaxed),
            total_latency_ns: entry.total_latency_ns.load(Ordering::Relaxed),
            write_count: entry.write_count.load(Ordering::Relaxed),
            min_latency_ns: entry.min_latency_ns.load(Ordering::Relaxed),
            max_latency_ns: entry.max_latency_ns.load(Ordering::Relaxed),
            error_count: entry.error_count.load(Ordering::Relaxed),
        })
    }

    /// Get statistics for all partitions
    pub fn get_all_partition_stats(&self) -> Vec<PartitionStats> {
        (0..self.partition_count)
            .filter_map(|id| self.get_partition_stats(id))
            .collect()
    }

    /// Detect partition imbalance (hot partitions)
    ///
    /// Returns alerts for any partition with >2x average event count.
    /// This is the SierraDB pattern for detecting skew and hot partitions.
    ///
    /// # Returns
    /// Vector of alerts for imbalanced partitions
    pub fn detect_partition_imbalance(&self) -> Vec<PartitionImbalanceAlert> {
        let mut alerts = Vec::new();
        let stats = self.get_all_partition_stats();

        // Calculate total and average event count
        let total_events: u64 = stats.iter().map(|s| s.event_count).sum();
        let active_partitions = stats.iter().filter(|s| s.event_count > 0).count();

        if active_partitions == 0 {
            return alerts;
        }

        let average_count = total_events as f64 / active_partitions as f64;
        let imbalance_threshold = 2.0; // SierraDB threshold: 2x average

        for stat in stats {
            if stat.event_count == 0 {
                continue;
            }

            let ratio = stat.event_count as f64 / average_count;

            if ratio > imbalance_threshold {
                alerts.push(PartitionImbalanceAlert {
                    partition_id: stat.partition_id,
                    event_count: stat.event_count,
                    average_count,
                    ratio_to_average: ratio,
                    message: format!(
                        "Partition {} has {:.1}x average load ({} events vs {:.0} avg)",
                        stat.partition_id, ratio, stat.event_count, average_count
                    ),
                    timestamp: chrono::Utc::now(),
                });
            }
        }

        alerts
    }

    /// Get partition count
    pub fn partition_count(&self) -> u32 {
        self.partition_count
    }

    /// Get total events across all partitions
    pub fn total_events(&self) -> u64 {
        self.partitions
            .iter()
            .map(|e| e.event_count.load(Ordering::Relaxed))
            .sum()
    }

    /// Get total errors across all partitions
    pub fn total_errors(&self) -> u64 {
        self.partitions
            .iter()
            .map(|e| e.error_count.load(Ordering::Relaxed))
            .sum()
    }

    /// Get uptime since metrics collection started
    pub fn uptime(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// Get the Prometheus registry for this partition metrics
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Encode metrics in Prometheus text format
    pub fn encode(&self) -> Result<String, Box<dyn std::error::Error>> {
        use prometheus::Encoder;
        let encoder = prometheus::TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer)?;
        Ok(String::from_utf8(buffer)?)
    }

    /// Get partition distribution as a map
    pub fn get_distribution(&self) -> HashMap<u32, u64> {
        self.partitions
            .iter()
            .enumerate()
            .map(|(id, entry)| (id as u32, entry.event_count.load(Ordering::Relaxed)))
            .collect()
    }

    /// Reset all partition metrics
    pub fn reset(&self) {
        for entry in &self.partitions {
            entry.event_count.store(0, Ordering::Relaxed);
            entry.total_latency_ns.store(0, Ordering::Relaxed);
            entry.write_count.store(0, Ordering::Relaxed);
            entry.min_latency_ns.store(u64::MAX, Ordering::Relaxed);
            entry.max_latency_ns.store(0, Ordering::Relaxed);
            entry.error_count.store(0, Ordering::Relaxed);
        }
    }
}

impl Default for PartitionMetrics {
    fn default() -> Self {
        Self::with_default_partitions()
    }
}

#[cfg(test)]
mod partition_tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_partition_metrics_creation() {
        let metrics = PartitionMetrics::new(32);
        assert_eq!(metrics.partition_count(), 32);
        assert_eq!(metrics.total_events(), 0);
        assert_eq!(metrics.total_errors(), 0);
    }

    #[test]
    fn test_partition_metrics_default() {
        let metrics = PartitionMetrics::default();
        assert_eq!(metrics.partition_count(), 32);
    }

    #[test]
    fn test_record_write() {
        let metrics = PartitionMetrics::new(32);

        metrics.record_write(0, Duration::from_micros(100));
        metrics.record_write(0, Duration::from_micros(200));
        metrics.record_write(1, Duration::from_micros(150));

        let stats0 = metrics.get_partition_stats(0).unwrap();
        assert_eq!(stats0.event_count, 2);
        assert_eq!(stats0.write_count, 2);

        let stats1 = metrics.get_partition_stats(1).unwrap();
        assert_eq!(stats1.event_count, 1);
    }

    #[test]
    fn test_record_batch_write() {
        let metrics = PartitionMetrics::new(32);

        metrics.record_batch_write(5, 100, Duration::from_millis(10));

        let stats = metrics.get_partition_stats(5).unwrap();
        assert_eq!(stats.event_count, 100);
        assert_eq!(stats.write_count, 1);
    }

    #[test]
    fn test_record_error() {
        let metrics = PartitionMetrics::new(32);

        metrics.record_error(3);
        metrics.record_error(3);
        metrics.record_error(5);

        let stats3 = metrics.get_partition_stats(3).unwrap();
        assert_eq!(stats3.error_count, 2);

        let stats5 = metrics.get_partition_stats(5).unwrap();
        assert_eq!(stats5.error_count, 1);

        assert_eq!(metrics.total_errors(), 3);
    }

    #[test]
    fn test_invalid_partition_id() {
        let metrics = PartitionMetrics::new(32);

        // Should not panic, just ignore invalid partition IDs
        metrics.record_write(100, Duration::from_micros(100));
        metrics.record_error(100);

        assert!(metrics.get_partition_stats(100).is_none());
    }

    #[test]
    fn test_latency_tracking() {
        let metrics = PartitionMetrics::new(32);

        metrics.record_write(0, Duration::from_micros(100));
        metrics.record_write(0, Duration::from_micros(200));
        metrics.record_write(0, Duration::from_micros(300));

        let stats = metrics.get_partition_stats(0).unwrap();
        assert_eq!(stats.min_latency_ns, 100_000); // 100 microseconds in nanoseconds
        assert_eq!(stats.max_latency_ns, 300_000); // 300 microseconds in nanoseconds

        let avg = stats.avg_latency().unwrap();
        assert_eq!(avg, Duration::from_micros(200)); // Average: 200 microseconds
    }

    #[test]
    fn test_detect_partition_imbalance_no_imbalance() {
        let metrics = PartitionMetrics::new(4);

        // Distribute events evenly
        for i in 0..4 {
            for _ in 0..100 {
                metrics.record_write(i, Duration::from_micros(100));
            }
        }

        let alerts = metrics.detect_partition_imbalance();
        assert!(
            alerts.is_empty(),
            "No alerts expected for balanced partitions"
        );
    }

    #[test]
    fn test_detect_partition_imbalance_hot_partition() {
        let metrics = PartitionMetrics::new(4);

        // Partition 0 gets 500 events, others get 100 each
        // Average = (500 + 100 + 100 + 100) / 4 = 200
        // Partition 0 ratio = 500/200 = 2.5x (>2x threshold)
        for _ in 0..500 {
            metrics.record_write(0, Duration::from_micros(100));
        }
        for i in 1..4 {
            for _ in 0..100 {
                metrics.record_write(i, Duration::from_micros(100));
            }
        }

        let alerts = metrics.detect_partition_imbalance();
        assert_eq!(alerts.len(), 1, "Expected one alert for hot partition");
        assert_eq!(alerts[0].partition_id, 0);
        assert!(alerts[0].ratio_to_average > 2.0);
    }

    #[test]
    fn test_detect_partition_imbalance_empty() {
        let metrics = PartitionMetrics::new(4);

        let alerts = metrics.detect_partition_imbalance();
        assert!(alerts.is_empty(), "No alerts expected for empty metrics");
    }

    #[test]
    fn test_get_all_partition_stats() {
        let metrics = PartitionMetrics::new(4);

        metrics.record_write(0, Duration::from_micros(100));
        metrics.record_write(2, Duration::from_micros(200));

        let all_stats = metrics.get_all_partition_stats();
        assert_eq!(all_stats.len(), 4);
        assert_eq!(all_stats[0].event_count, 1);
        assert_eq!(all_stats[1].event_count, 0);
        assert_eq!(all_stats[2].event_count, 1);
        assert_eq!(all_stats[3].event_count, 0);
    }

    #[test]
    fn test_prometheus_encoding() {
        let metrics = PartitionMetrics::new(4);

        metrics.record_write(0, Duration::from_micros(100));
        metrics.record_write(1, Duration::from_micros(200));
        metrics.record_error(0);

        let encoded = metrics.encode().unwrap();

        assert!(encoded.contains("allsource_partition_events_total"));
        assert!(encoded.contains("allsource_partition_write_latency"));
        assert!(encoded.contains("allsource_partition_errors_total"));
    }

    #[test]
    fn test_reset() {
        let metrics = PartitionMetrics::new(4);

        metrics.record_write(0, Duration::from_micros(100));
        metrics.record_error(1);

        assert_eq!(metrics.total_events(), 1);
        assert_eq!(metrics.total_errors(), 1);

        metrics.reset();

        assert_eq!(metrics.total_events(), 0);
        assert_eq!(metrics.total_errors(), 0);
    }

    #[test]
    fn test_concurrent_writes() {
        let metrics = Arc::new(PartitionMetrics::new(32));
        let mut handles = vec![];

        // Spawn 8 threads, each writing 1000 events to random partitions
        for _ in 0..8 {
            let metrics_clone = metrics.clone();
            let handle = thread::spawn(move || {
                for i in 0..1000 {
                    let partition_id = (i % 32) as u32;
                    metrics_clone.record_write(partition_id, Duration::from_micros(100));
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(metrics.total_events(), 8000);
    }

    #[test]
    fn test_get_distribution() {
        let metrics = PartitionMetrics::new(4);

        metrics.record_write(0, Duration::from_micros(100));
        metrics.record_write(0, Duration::from_micros(100));
        metrics.record_write(2, Duration::from_micros(100));

        let distribution = metrics.get_distribution();

        assert_eq!(distribution.get(&0), Some(&2));
        assert_eq!(distribution.get(&1), Some(&0));
        assert_eq!(distribution.get(&2), Some(&1));
        assert_eq!(distribution.get(&3), Some(&0));
    }

    #[test]
    fn test_partition_stats_avg_latency_none() {
        let stats = PartitionStats::new(0);
        assert!(stats.avg_latency().is_none());
    }

    #[test]
    fn test_alert_message_format() {
        let metrics = PartitionMetrics::new(4);

        // Create imbalanced scenario
        for _ in 0..1000 {
            metrics.record_write(0, Duration::from_micros(100));
        }
        for i in 1..4 {
            for _ in 0..100 {
                metrics.record_write(i, Duration::from_micros(100));
            }
        }

        let alerts = metrics.detect_partition_imbalance();
        assert!(!alerts.is_empty());

        let alert = &alerts[0];
        assert!(alert.message.contains("Partition 0"));
        assert!(alert.message.contains("average load"));
    }

    #[test]
    fn test_uptime() {
        let metrics = PartitionMetrics::new(4);

        // Sleep a bit to ensure non-zero uptime
        thread::sleep(Duration::from_millis(10));

        let uptime = metrics.uptime();
        assert!(uptime.as_millis() >= 10);
    }
}
