# Critical Bugs Fixed - AllSource Core

> **Note (2026-02-16)**: The PostgreSQL (`postgres_event_stream_repository.rs`) and RocksDB (`rocksdb_event_stream_repository.rs`) storage backends referenced below are **feature-gated** and not used in the default build. The default storage backend is WAL + Parquet + DashMap. These feature-gated repos exist for the AllFrame embedded integration use case.

**Date**: November 30, 2025
**Status**: ✅ RESOLVED
**Impact**: AllFrame integration **UNBLOCKED**
**Commit**: Latest (November 30, 2025)

---

## Executive Summary

Three critical compilation errors in `allsource-core` (commit dd22949a) were preventing AllFrame from building with the `cqrs-allsource` feature enabled. All issues have been **RESOLVED** and the library now compiles successfully.

### Impact Before Fixes
- ❌ Cannot build AllFrame with `--all-features`
- ❌ Cannot use AllSource embedded database backend
- ❌ Cannot test multi-tenant event streaming
- ❌ Single-binary deployments blocked

### Impact After Fixes  
- ✅ Full compilation success (`cargo build --lib`)
- ✅ All trait implementations complete
- ✅ Proper error conversion chain
- ✅ AllFrame integration unblocked

---

## Bugs Fixed

### Bug 1: Missing Trait Methods (HIGH PRIORITY) ✅ FIXED

**Error**: `EventStreamRepository` trait declared two methods not implemented by backend repositories

**Location**:
- `apps/core/src/infrastructure/repositories/postgres_event_stream_repository.rs:153`
- `apps/core/src/infrastructure/repositories/rocksdb_event_stream_repository.rs:200`

**Missing Methods**:
```rust
async fn get_streams_by_tenant(&self, tenant_id: &TenantId) -> Result<Vec<EventStream>>;
async fn count_streams_by_tenant(&self, tenant_id: &TenantId) -> Result<usize>;
```

**Root Cause**:  
Trait definition in `apps/core/src/domain/repositories/event_stream_repository.rs` added tenant-scoped query methods (lines 108-114) but implementations were not added to PostgreSQL and RocksDB repositories.

**Fix Applied**:

**PostgreSQL Implementation** (`postgres_event_stream_repository.rs:443-474`):
```rust
async fn get_streams_by_tenant(&self, tenant_id: &TenantId) -> Result<Vec<EventStream>> {
    // Query to find all stream_ids that have events belonging to the tenant
    let stream_ids: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT stream_id FROM events WHERE tenant_id = $1 ORDER BY stream_id"
    )
    .bind(tenant_id.as_str())
    .fetch_all(&self.pool)
    .await?;

    // Load each stream
    let mut streams = Vec::new();
    for stream_id_str in stream_ids {
        let stream_id = EntityId::new(stream_id_str)?;
        if let Some(stream) = self.load_stream(&stream_id).await? {
            streams.push(stream);
        }
    }

    Ok(streams)
}

async fn count_streams_by_tenant(&self, tenant_id: &TenantId) -> Result<usize> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT stream_id) FROM events WHERE tenant_id = $1"
    )
    .bind(tenant_id.as_str())
    .fetch_one(&self.pool)
    .await?;

    Ok(count as usize)
}
```

**RocksDB Implementation** (`rocksdb_event_stream_repository.rs:405-462`):
```rust
async fn get_streams_by_tenant(&self, tenant_id: &TenantId) -> Result<Vec<EventStream>> {
    use std::collections::HashSet;

    let cf_events = self.db.cf_handle(CF_EVENTS)
        .ok_or_else(|| AllSourceError::StorageError("Events CF not found".to_string()))?;

    // Find all stream_ids that have events belonging to this tenant
    let mut stream_ids = HashSet::new();
    let iter = self.db.iterator_cf(cf_events, IteratorMode::Start);

    for item in iter {
        let (_, value) = item.map_err(|e| AllSourceError::StorageError(format!("Iterator error: {}", e)))?;

        if let Ok(event) = Self::deserialize_event(&value) {
            if event.tenant_id().as_str() == tenant_id.as_str() {
                stream_ids.insert(event.entity_id().as_str().to_string());
            }
        }
    }

    // Load each stream
    let mut streams = Vec::new();
    for stream_id_str in stream_ids {
        let entity_id = EntityId::new(stream_id_str)?;
        if let Some(stream) = self.load_stream(&entity_id).await? {
            streams.push(stream);
        }
    }

    streams.sort_by(|a, b| a.stream_id().as_str().cmp(b.stream_id().as_str()));
    Ok(streams)
}

async fn count_streams_by_tenant(&self, tenant_id: &TenantId) -> Result<usize> {
    use std::collections::HashSet;

    let cf_events = self.db.cf_handle(CF_EVENTS)
        .ok_or_else(|| AllSourceError::StorageError("Events CF not found".to_string()))?;

    // Find distinct stream_ids that have events belonging to this tenant
    let mut stream_ids = HashSet::new();
    let iter = self.db.iterator_cf(cf_events, IteratorMode::Start);

    for item in iter {
        let (_, value) = item.map_err(|e| AllSourceError::StorageError(format!("Iterator error: {}", e)))?;

        if let Ok(event) = Self::deserialize_event(&value) {
            if event.tenant_id().as_str() == tenant_id.as_str() {
                stream_ids.insert(event.entity_id().as_str().to_string());
            }
        }
    }

    Ok(stream_ids.len())
}
```

**Use Cases Unblocked**:
- ✅ Multi-tenant event stream queries
- ✅ Tenant-based event stream counting
- ✅ SaaS tenant isolation
- ✅ Tenant usage metrics and billing

---

### Bug 2: Method Naming Mismatch (MEDIUM PRIORITY) ✅ FIXED

**Error**: Code calls `expected_version()` getter but only `expect_version()` setter exists

**Location**: `apps/core/src/infrastructure/repositories/postgres_event_stream_repository.rs:237`

**Code Calling Non-Existent Method**:
```rust
if let Some(expected) = stream.expected_version() {  // ERROR: method doesn't exist
    if expected != current_version as u64 {
        return Err(AllSourceError::ConcurrencyError(...));
    }
}
```

**Available Method** (`domain/entities/event_stream.rs:151`):
```rust
pub fn expect_version(&mut self, version: u64) {  // Setter, not getter
    self.expected_version = Some(version);
}
```

**Root Cause**:  
`EventStream` entity had a setter `expect_version()` for optimistic locking but no corresponding getter `expected_version()` to read the value.

**Fix Applied** (`apps/core/src/domain/entities/event_stream.rs:216-218`):
```rust
pub fn expected_version(&self) -> Option<u64> {
    self.expected_version
}
```

**Use Cases Unblocked**:
- ✅ Optimistic locking version checks in PostgreSQL repository
- ✅ Optimistic locking version checks in RocksDB repository
- ✅ Concurrent write conflict detection
- ✅ Production observability of version expectations

---

### Bug 3: Missing Error Conversion (HIGH PRIORITY) ✅ FIXED

**Error**: `AllSourceError` doesn't implement `From<sqlx::Error>`

**Location**: Multiple locations in `postgres_event_stream_repository.rs` (lines 328-333, 372-375, etc.)

**Code Failing**:
```rust
let partition_id: i32 = row.try_get("partition_id")?;  // ERROR: ? can't convert sqlx::Error
let current_version: i64 = row.try_get("current_version")?;
let watermark: i64 = row.try_get("watermark")?;
```

**Current `AllSourceError`** (`apps/core/src/error.rs:6`):
```rust
pub enum AllSourceError {
    // Has From implementations for:
    // - arrow::error::ArrowError (line 56)
    // - parquet::errors::ParquetError (line 62)
    // - serde_json::Error (line 32)
    // Missing: sqlx::Error ❌
}
```

**Root Cause**:  
PostgreSQL repository uses `sqlx::Row::try_get()` which returns `Result<T, sqlx::Error>`. The `?` operator requires `From<sqlx::Error>` to convert to `AllSourceError`, but this conversion was not implemented.

**Fix Applied** (`apps/core/src/error.rs:68-73`):
```rust
#[cfg(feature = "postgres")]
impl From<sqlx::Error> for AllSourceError {
    fn from(err: sqlx::Error) -> Self {
        AllSourceError::StorageError(format!("Database error: {}", err))
    }
}
```

**Use Cases Unblocked**:
- ✅ Database error propagation from sqlx
- ✅ Production observability with proper error context
- ✅ Debugging database issues
- ✅ SRE alerting and monitoring

---

## Additional Test Fixes (Non-Blocking)

Several test files had missing imports that prevented `cargo test --lib` from compiling. These were **non-critical** (library builds fine, only tests affected) but have been fixed:

### Test Fix 1: DTO Imports in `manage_schema.rs`

**File**: `apps/core/src/application/use_cases/manage_schema.rs:1-4`

**Change**:
```rust
// Before
use crate::application::dto::{
    RegisterSchemaRequest, RegisterSchemaResponse, UpdateSchemaRequest, ListSchemasResponse,
    SchemaDto,
};

// After
use crate::application::dto::{
    RegisterSchemaRequest, RegisterSchemaResponse, UpdateSchemaRequest, ListSchemasResponse,
    SchemaDto, CompatibilityModeDto,  // Added for tests
};
```

### Test Fix 2: DTO Imports in `manage_projection.rs`

**File**: `apps/core/src/application/use_cases/manage_projection.rs:1-4`

**Change**:
```rust
// Before
use crate::application::dto::{
    CreateProjectionRequest, CreateProjectionResponse, UpdateProjectionRequest,
    ListProjectionsResponse, ProjectionDto,
};

// After
use crate::application::dto::{
    CreateProjectionRequest, CreateProjectionResponse, UpdateProjectionRequest,
    ListProjectionsResponse, ProjectionDto, ProjectionTypeDto, ProjectionConfigDto,  // Added for tests
};
```

### Test Fix 3: Error Import in `in_memory_event_stream_repository.rs`

**File**: `apps/core/src/infrastructure/repositories/in_memory_event_stream_repository.rs:9`

**Change**:
```rust
// Before
use crate::error::Result;

// After
use crate::error::{Result, AllSourceError};  // Added for test assertions
```

### Test Fix 4: TenantId Import in `anomaly_detection.rs`

**File**: `apps/core/src/security/anomaly_detection.rs:11-13`

**Change**:
```rust
// Before
use crate::domain::entities::{AuditEvent, AuditAction, AuditOutcome};
use crate::error::Result;

// After
use crate::domain::entities::{AuditEvent, AuditAction, AuditOutcome};
use crate::domain::value_objects::TenantId;  // Added for tests
use crate::error::Result;
```

---

## Verification

### Build Verification ✅
```bash
cd apps/core
cargo build --lib
# Output: Finished `dev` profile [unoptimized + debuginfo] target(s) in 10.63s
```

### Feature Verification ✅
```bash
# PostgreSQL feature (if enabled)
cargo build --lib --features postgres
# Success - all PostgreSQL repository methods compile

# RocksDB feature (if enabled)
cargo build --lib --features rocksdb-storage
# Success - all RocksDB repository methods compile
```

### AllFrame Integration ✅
AllFrame can now:
1. Build with `--features cqrs-allsource`
2. Use AllSource embedded database
3. Query events by tenant
4. Deploy as single binary

---

## Files Modified

### Core Fixes (3 files)
1. `apps/core/src/domain/entities/event_stream.rs` - Added `expected_version()` getter
2. `apps/core/src/error.rs` - Added `From<sqlx::Error>` implementation  
3. `apps/core/src/infrastructure/repositories/postgres_event_stream_repository.rs` - Implemented tenant methods
4. `apps/core/src/infrastructure/repositories/rocksdb_event_stream_repository.rs` - Implemented tenant methods

### Test Fixes (4 files)
5. `apps/core/src/application/use_cases/manage_schema.rs` - Added DTO imports
6. `apps/core/src/application/use_cases/manage_projection.rs` - Added DTO imports
7. `apps/core/src/infrastructure/repositories/in_memory_event_stream_repository.rs` - Added error import
8. `apps/core/src/security/anomaly_detection.rs` - Added TenantId import

---

## Timeline

- **November 27, 2025**: AllFrame team reports compilation errors
- **November 30, 2025**: Issues analyzed and prioritized
- **November 30, 2025**: All critical fixes implemented and verified

---

## Lessons Learned

### API Design
- **Lesson**: Trait methods must be implemented by ALL implementations before merging
- **Action**: Add CI check to verify all trait impls are complete

### Error Handling
- **Lesson**: Error conversion chains must be complete for all backend types
- **Action**: Document error conversion requirements in contributing guide

### Testing
- **Lesson**: Feature-gated code (postgres, rocksdb) must be tested in CI
- **Action**: Add CI matrix to test all feature combinations

---

## Related Documentation

- [Tenant Architecture](TENANT_ARCHITECTURE.md) - Multi-tenancy design
- [Architecture Optimization](../archive/2026-11-23/current/ARCHITECTURE_OPTIMIZATION.md) - Overall architecture
- [Performance Guide](PERFORMANCE.md) - Performance characteristics

---

**Document Status**: ✅ CURRENT  
**Version**: 1.0  
**Last Updated**: November 30, 2025
