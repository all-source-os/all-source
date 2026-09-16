// Persistence infrastructure layer
// Contains storage implementations, WAL, snapshots, compaction, and indexing

pub mod arena_pool;
pub mod backup;
pub mod batch_processor;
pub mod cold_tier;
#[cfg(feature = "cold-tier-s3")]
pub mod cold_tier_s3;
pub mod compaction;
pub mod index;
pub mod lock_free;
pub mod performance;
pub mod simd_filter;
pub mod simd_json;
pub mod snapshot;
pub mod storage;
pub mod storage_integrity;
pub mod system_bootstrap;
pub mod system_store;
pub mod tenant_loader;
pub mod wal;

// Re-exports for convenience
pub use arena_pool::{
    ArenaPoolStats, PooledArena, ScopedArena, SizedBufferPool, arena_stats, get_arena,
    get_arena_with_capacity,
};
pub use backup::*;
pub use batch_processor::{
    ArenaBatchBuffer, BatchProcessor, BatchProcessorConfig, BatchProcessorStats, BatchResult,
    RawEventData,
};
pub use cold_tier::{ArchiveTarget, LocalFsArchive};
pub use compaction::{CompactionConfig, CompactionManager, CompactionResult, CompactionStrategy};
pub use index::{EventIndex, IndexEntry};
pub use lock_free::{
    LockFreeEventQueue, LockFreeMetrics, MetricsSnapshot, ShardedEventQueue, ShardedQueueStats,
};
pub use performance::{BatchWriter, MemoryPool, PerformanceMetrics};
pub use simd_filter::{
    FilterPredicate, SimdEventFilter, SimdFilterStats, filter_events_simd,
    filter_events_simd_indices,
};
pub use simd_json::{BatchEventParser, SimdJsonError, SimdJsonParser, SimdJsonStats, ZeroCopyJson};
pub use snapshot::{
    CreateSnapshotRequest, CreateSnapshotResponse, ListSnapshotsRequest, ListSnapshotsResponse,
    Snapshot, SnapshotConfig, SnapshotInfo, SnapshotManager, SnapshotType,
};
pub use storage::{
    BatchWriteResult, BatchWriteStats, DEFAULT_BATCH_SIZE, DEFAULT_FLUSH_TIMEOUT_MS,
    MigrationReport, ParquetStorage, ParquetStorageConfig,
};
pub use storage_integrity::{IntegrityCheckResult, StorageIntegrity};
pub use system_bootstrap::{SystemBootstrap, SystemRepositories};
pub use system_store::SystemMetadataStore;
pub use tenant_loader::{DEFAULT_LOAD_TIMEOUT, TenantLoader};
pub use wal::{WALConfig, WriteAheadLog};
