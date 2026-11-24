---
title: "AllSource Event Store - Comprehensive Features and Capabilities"
status: CURRENT
last_updated: 2026-11-24
version: "1.0"
related:
  - "../archive/2026-11-23/current/CLEAN_ARCHITECTURE.md"
  - "./PERFORMANCE.md"
  - "./TENANT_ARCHITECTURE.md"
---

# AllSource Event Store - Comprehensive Features and Capabilities

## Executive Summary

AllSource is a **high-performance, distributed event store** built on a Rust-based core service with a Go control plane orchestration layer. It supports event sourcing, CQRS, and event-driven architectures with enterprise-grade features including multi-tenancy, schema management, projections, and advanced analytics.

**Performance**: Ingests events at 469K/sec, with full query and replay capabilities.

---

## Part 1: Core Features (Rust Event Store Service)

### 1.1 Event Management

#### Create/Ingest Events
- **Single Event Ingestion** (`POST /api/v1/events`)
  - Create immutable events with event type, entity ID, payload, optional metadata
  - Automatic UUID generation and timestamping
  - Validation using domain value objects (EventType, EntityId, TenantId)
  - Support for JSON payloads of arbitrary complexity

- **Batch Event Ingestion** (via use case)
  - Optimized batch insertion of multiple events
  - Transactional guarantees
  - Atomic operations on failure

#### Event Structure
```
Event {
  id: UUID,
  event_type: String (e.g., "user.created", "order.placed"),
  entity_id: String (identifies the aggregate/entity),
  tenant_id: String (multi-tenant isolation),
  payload: JSON (arbitrary event data),
  timestamp: DateTime<UTC>,
  metadata: Optional<JSON> (e.g., source, correlation IDs),
  version: i64 (immutable sequence number)
}
```

#### Domain Validation
- Event type format validation (lowercase, dot-notation)
- Entity ID cannot be empty
- Tenant ID cannot be empty
- Timestamps cannot be in future
- Immutable after creation

### 1.2 Event Querying

#### Query Patterns Available
1. **By Entity ID** - Get all events for a specific entity (aggregate root)
2. **By Event Type** - Filter events by type (e.g., all "user.created" events)
3. **By Time Range** - Query events between timestamps
4. **By Entity as of Time** - Time-travel queries (reconstruct state at specific point)
5. **Combination Filters** - Mix entity, type, and time constraints

#### Query Features
- Pagination with limit support
- Optional since/until date filters
- Multi-tenant isolation (queries filtered by tenant)
- Optimized repository pattern with specialized implementations
- WebSocket streaming support for real-time event subscriptions (`GET /api/v1/events/stream`)

### 1.3 Event Stream Management

#### Event Stream Aggregate
- **Gapless Version Numbers**: SierraDB-inspired watermark pattern ensures no gaps
- **Optimistic Locking**: Detect concurrent modifications with expected_version
- **Partition Assignment**: Events automatically partitioned for scalability
- **Tenant Isolation**: Stream-level validation ensures same tenant ownership

#### Stream Operations
- Append events with version consistency
- Get events from specific version onwards
- Validate stream integrity (gapless sequences)
- Tenant consistency checks

### 1.4 Entity State Reconstruction

#### State Building
- **Get Entity State** (`GET /api/v1/entities/{entity_id}/state`)
  - Reconstruct current state by replaying all events
  - JSON object representing aggregate state
  - Time-travel queries with `?as_of=` parameter

- **Get Entity Snapshot** (`GET /api/v1/entities/{entity_id}/snapshot`)
  - Retrieve cached snapshot (faster than full replay)
  - Multiple snapshots per entity
  - Metadata including event count and size

### 1.5 Event Streaming (WebSocket)

- Real-time event subscriptions via WebSocket
- Server-sent events for push notifications
- Backpressure handling
- Multiple concurrent subscribers

---

## Part 2: Projections (Materialized Views)

### 2.1 Projection Types

1. **Entity Snapshot** - Maintains current state by entity ID
2. **Event Counter** - Counts events by type
3. **Custom** - User-defined projection logic
4. **Time Series** - Time-based aggregation
5. **Funnel** - Multi-step event sequences analysis

### 2.2 Projection Lifecycle

#### Status States
- **Created** - Projection defined but not started
- **Running** - Actively processing events
- **Paused** - Temporarily suspended
- **Failed** - Error encountered
- **Stopped** - Halted
- **Rebuilding** - Reconstructing from scratch

#### Operations
- **Create Projection** (`POST` via use case)
  - Name, type, tenant, event type filters, config
  - Description and custom metadata
  - Batch size, checkpoint settings, parallelism config

- **Start/Stop/Pause** - Control projection lifecycle
- **Rebuild** - Reset and reprocess all events
- **Update** - Modify config, description, event filters

### 2.3 Projection Configuration

```
ProjectionConfig {
  batch_size: usize,              // Events per batch
  enable_checkpoints: bool,       // Periodic state checkpoints
  checkpoint_interval: usize,     // Events between checkpoints
  parallel_processing: bool,      // Process in parallel
  max_concurrency: usize          // Max concurrent handlers
}
```

### 2.4 Projection Statistics

- Events processed count
- Last processed timestamp
- Checkpoint timestamps
- Error count and last error time
- Processing time metrics (total and average)

### 2.5 Event Type Filtering

- Add/remove specific event types to process
- Empty filter means process all events
- Validation ensures no duplicates
- Can update filters on running projections

---

## Part 3: Schema Management & Validation

### 3.1 Schema Registry

#### Features
- Register JSON Schemas with subjects (event types)
- Versioning system (v1, v2, v3, ...)
- Schema evolution with compatibility modes
- Metadata: description, tags, creation timestamp

#### Compatibility Modes
- **None** - No compatibility checking
- **Backward** - New schema backward compatible (new fields optional)
- **Forward** - New schema forward compatible (old fields preserved)
- **Full** - Both backward and forward compatible

### 3.2 Operations

- **Register Schema** (`POST /api/v1/schemas`)
  - Subject, schema definition, optional description and tags
  - Returns schema ID and version

- **Get Schema** (`GET /api/v1/schemas/{subject}`)
  - Retrieve latest schema version for subject

- **List Schema Versions** (`GET /api/v1/schemas/{subject}/versions`)
  - All versions of a schema

- **List Subjects** (`GET /api/v1/schemas`)
  - All registered schema subjects

- **Validate Event** (`POST /api/v1/schemas/validate`)
  - Validate payload against schema
  - Returns validation status and errors
  - Supports specific version targeting

- **Set Compatibility Mode** (`PUT /api/v1/schemas/{subject}/compatibility`)
  - Update compatibility rules for a subject

---

## Part 4: Snapshots & Compaction

### 4.1 Snapshots

#### Snapshot Structure
```
Snapshot {
  id: UUID,
  entity_id: String,
  state: JSON,                    // Entity state at point in time
  created_at: DateTime<UTC>,
  as_of: DateTime<UTC>,          // Timestamp of events included
  event_count: usize,             // Events processed for snapshot
  metadata: {
    snapshot_type: Manual|Automatic|OnDemand,
    size_bytes: usize,
    version: u32
  }
}
```

#### Operations
- **Create Snapshot** (`POST /api/v1/operations/snapshots`, Control Plane)
  - Create snapshot for entity or all entities
  - Compresses state for storage efficiency

- **List Snapshots** (`GET /api/v1/operations/snapshots`, Control Plane)
  - Optional entity_id filter
  - Lists all snapshots with timestamps and metadata

### 4.2 Compaction

#### Compaction Manager
- Automatic background compaction of Parquet storage files
- Configurable triggers and strategies

#### Strategies
- **SizeBased** - Trigger when small files accumulate
- **TimeBased** - Trigger based on file age
- **FullCompaction** - Merge all files into single file

#### Configuration
- Minimum files to compact (default: 3)
- Target file size (default: 128 MB)
- Max file size (default: 256 MB)
- Small file threshold (default: 10 MB)
- Auto-compaction interval (default: 1 hour)

#### Operations
- **Trigger Compaction** (`POST /api/v1/compaction/trigger`)
- **Get Compaction Stats** (`GET /api/v1/compaction/stats`)
  - Total compactions, files processed, space saved
  - Performance metrics

---

## Part 5: Replay & Time-Travel Queries

### 5.1 Replay System

#### Purpose
- Rebuild projections from scratch
- Test event processing logic
- Fix incorrect projection state
- Migrate between storage backends

#### Configuration
```
ReplayConfig {
  batch_size: usize,              // Events per batch
  parallel: bool,                 // Parallel processing
  workers: usize,                 // Worker threads
  emit_progress: bool,            // Report progress
  progress_interval: usize        // Report every N events
}
```

#### Replay Filters
- Optional projection name (rebuild specific projection)
- From timestamp (start from specific point)
- To timestamp (end at specific point)
- Entity ID filter
- Event type filter

#### Status Tracking
- **Pending** - Queued but not started
- **Running** - Currently processing
- **Completed** - Successfully finished
- **Failed** - Error occurred
- **Cancelled** - User cancelled

#### Operations
- **Start Replay** (`POST /api/v1/replay`)
- **Get Replay Progress** (`GET /api/v1/replay/{replay_id}`)
- **List Active Replays** (`GET /api/v1/replay`)
- **Cancel Replay** (`POST /api/v1/replay/{replay_id}/cancel`)
- **Delete Replay** (`DELETE /api/v1/replay/{replay_id}`)

---

## Part 6: Stream Processing Pipelines

### 6.1 Pipeline Architecture

#### Operators
1. **Filter** - Condition-based event filtering
   - Field path, expected value, operator (eq, ne, gt, lt, contains)

2. **Map** - Transform event payload
   - Field transformations with expressions

3. **Reduce** - Aggregation (sum, count, avg, min, max)
   - Optional group-by field

4. **Window** - Time-based aggregations
   - Window types: Tumbling, Sliding, Session
   - Configurable size and slide intervals

5. **Enrich** - Add external data
   - Data source specification
   - Fields to merge

6. **Branch** - Split stream by condition
   - Condition field and branch mapping

### 6.2 Window Types

- **Tumbling** - Non-overlapping consecutive windows
- **Sliding** - Overlapping windows with slide interval
- **Session** - Activity-based windows with timeout

### 6.3 Operations
- **Register Pipeline** (`POST /api/v1/pipelines`)
- **List Pipelines** (`GET /api/v1/pipelines`)
- **Get Pipeline** (`GET /api/v1/pipelines/{pipeline_id}`)
- **Delete Pipeline** (`DELETE /api/v1/pipelines/{pipeline_id}`)
- **Get Pipeline Stats** (`GET /api/v1/pipelines/{pipeline_id}/stats`)
- **Reset Pipeline** (`PUT /api/v1/pipelines/{pipeline_id}/reset`)
- **Get All Pipeline Stats** (`GET /api/v1/pipelines/stats`)

---

## Part 7: Advanced Analytics

### 7.1 Analytics Engine

#### Frequency Analysis
- **Event Frequency** (`GET /api/v1/analytics/frequency`)
- Time-bucketed event counts
- Configurable time windows (minute, hour, day, week, month)
- Filters: entity_id, event_type, time range
- Returns: Array of TimeBucket {timestamp, count}

#### Statistical Summary
- **Stats Summary** (`GET /api/v1/analytics/summary`)
- Total events count
- Unique entities count
- Event type distribution
- Entity event counts
- Time range metrics

#### Correlation Analysis
- **Event Correlation** (`GET /api/v1/analytics/correlation`)
- Identify related events across entities
- Correlation percentage
- Example correlated pairs
- Useful for understanding event relationships

### 7.2 Time Windows

Available granularities:
- Minute (1 min precision)
- Hour (hourly buckets)
- Day (daily buckets)
- Week (weekly buckets)
- Month (monthly buckets with 30-day intervals)

---

## Part 8: Multi-Tenancy & Isolation

### 8.1 Tenant Model

#### Structure
```
Tenant {
  id: UUID,
  name: String,
  status: Active|Suspended|Deleted,
  tier: Free|Standard|Professional|Enterprise,
  quotas: {
    max_events_per_day: u64,
    max_storage_bytes: u64,
    max_queries_per_hour: u64,
    max_api_keys: u32,
    max_projections: u32,
    max_pipelines: u32
  },
  created_at: DateTime<UTC>,
  metadata: JSON
}
```

#### Tier System
1. **Free Tier**
   - 10K events/day
   - 1 GB storage
   - 1K queries/hour
   - 2 API keys, 5 projections, 2 pipelines

2. **Standard Tier**
   - 1M events/day
   - 10 GB storage
   - 100K queries/hour
   - 10 API keys, 50 projections, 20 pipelines

3. **Professional Tier**
   - 1M events/day
   - 100 GB storage
   - 100K queries/hour
   - 25 API keys, 100 projections, 50 pipelines

4. **Enterprise Tier**
   - Unlimited

### 8.2 Isolation Features
- Events filtered by tenant_id on all queries
- Event streams validate tenant consistency
- Projections scoped to tenant
- Schemas accessible per tenant
- Audit logging of cross-tenant operations
- Request-level tenant validation

### 8.3 Tenant Operations
- Create tenant with quotas and tier
- List tenants
- Get tenant details
- Update quotas
- Suspend/activate tenants

---

## Part 9: Security & Authentication

### 9.1 RBAC (Role-Based Access Control)

#### Roles
1. **Admin** - Full system access
2. **Developer** - Read/write events, manage schemas, pipelines
3. **ReadOnly** - Read-only event access and metrics
4. **ServiceAccount** - Programmatic access for services

#### Permissions
- Read (events, schemas, metrics)
- Write (ingest events)
- Admin (all operations)
- Metrics (access metrics endpoints)
- ManageSchemas (schema operations)
- ManagePipelines (pipeline operations)
- ManageTenants (tenant management)

### 9.2 JWT Authentication

#### Claims Structure
```
Claims {
  sub: String,           // Subject (user/key ID)
  tenant_id: String,     // Tenant ID
  role: Role,           // User role
  exp: i64,             // Expiration (Unix timestamp)
  iat: i64,             // Issued at
  iss: String           // Issuer
}
```

### 9.3 API Key Management
- Generate API keys per tenant
- Key rotation support
- Automatic expiration
- Rate limiting per key
- Audit trail of API key usage

### 9.4 Security Features
- JWT token validation on protected endpoints
- Middleware-based authentication
- Optional auth (public endpoints)
- Rate limiting middleware
- IP filtering capabilities
- Tenant isolation validation
- Request ID tracking
- Security headers injection

### 9.5 Data Protection
- Encryption at rest (KMS support)
- Encryption in transit (TLS)
- Encryption key management
- Field-level encryption option
- Secure password hashing (Argon2)

---

## Part 10: Advanced Security Features

### 10.1 Anomaly Detection
- Detects unusual access patterns
- ML-based outlier identification
- Real-time alerting
- Configurable thresholds
- Historical baseline building

### 10.2 Adaptive Rate Limiting
- Per-tenant rate limits
- Per-key rate limits
- Backoff strategies
- Quota tracking
- Usage analytics

### 10.3 KMS (Key Management System)
- Centralized encryption key management
- Key rotation policies
- HSM integration support
- Key audit logging
- Multi-region key distribution

### 10.4 Audit Logging
```
AuditEvent {
  id: UUID,
  timestamp: DateTime<UTC>,
  user_id: String,
  tenant_id: String,
  action: String,        // Created, Updated, Deleted, Read
  resource: String,      // Event, Projection, Schema, etc.
  resource_id: String,
  status: Success|Failure,
  details: JSON,
  ip_address: String,
  user_agent: String
}
```

- Immutable audit logs
- Searchable by user, tenant, action, resource
- Retention policies
- Compliance reporting

---

## Part 11: Monitoring & Metrics

### 11.1 Prometheus Metrics (`GET /metrics`)

#### Event Store Metrics
- `allsource_events_ingested_total` - Total events ingested
- `allsource_events_ingested_duration_seconds` - Ingest latency
- `allsource_events_queried_total` - Total events queried
- `allsource_entities_total` - Unique entities
- `allsource_storage_bytes` - Total storage used

#### Projection Metrics
- `allsource_projection_events_processed_total` - Events processed
- `allsource_projection_processing_time_seconds` - Processing latency
- `allsource_projection_errors_total` - Processing errors

#### Query Metrics
- `allsource_query_duration_seconds` - Query latency by query type
- `allsource_query_cache_hits_total` - Cache hit rate

#### System Metrics
- `allsource_memory_bytes` - Memory usage
- `allsource_goroutines` - Active goroutines
- `allsource_storage_files` - Number of storage files

### 11.2 Aggregated Metrics (`GET /api/v1/stats`)

Returns:
```
{
  total_events: usize,
  total_entities: usize,
  total_projections: usize,
  total_schemas: usize,
  uptime_seconds: u64,
  memory_used_bytes: u64,
  storage_used_bytes: u64,
  ingest_rate_per_sec: f64,
  query_rate_per_sec: f64
}
```

### 11.3 Health Checks

- **GET /health** - Control plane health
- **GET /health/core** - Core service health
- Includes version, timestamp, status

---

## Part 12: Backup & Disaster Recovery

### 12.1 Backup Types

#### Full Backup
- Complete snapshot of all events
- Independent restoration
- Baseline for incremental backups

#### Incremental Backup
- Only new events since last backup
- References parent backup
- Storage efficient

### 12.2 Backup Features

#### Metadata
```
BackupMetadata {
  backup_id: String,
  created_at: DateTime<UTC>,
  backup_type: Full|Incremental,
  event_count: u64,
  size_bytes: u64,
  checksum: String,
  from_sequence: Option<u64>,
  to_sequence: u64,
  compressed: bool
}
```

#### Storage
- Compressed format (gzip)
- Filesystem or S3-compatible storage
- Configurable compression level
- Post-backup verification

### 12.3 Operations
- Create full/incremental backups
- Restore from specific backup
- List backup history
- Verify backup integrity
- Delete old backups

---

## Part 13: Control Plane (Go Service)

### 13.1 Purpose

Lightweight orchestration layer above the Rust core providing:
- Cluster management and coordination
- Metrics aggregation from multiple nodes
- Operation orchestration (snapshots, replays)
- Health monitoring
- Management APIs for operators

### 13.2 Control Plane Endpoints

#### Health & Status
- `GET /health` - Control plane health
- `GET /health/core` - Core service health
- `GET /api/v1/cluster/status` - Full cluster topology and status

#### Metrics & Monitoring
- `GET /metrics` - Prometheus metrics
- `GET /api/v1/metrics/json` - JSON metrics format

#### Operations
- `POST /api/v1/operations/snapshots` - Coordinate snapshot creation
- `POST /api/v1/operations/replay` - Coordinate replay operations

#### Management (Future)
- Tenant management endpoints
- Policy evaluation endpoints
- Authorization endpoints

### 13.3 Cluster Status Response

```json
{
  "cluster_id": "allsource-demo",
  "nodes": [
    {
      "id": "core-1",
      "type": "event-store",
      "status": "healthy",
      "url": "http://localhost:3900",
      "stats": {
        "total_events": 1234,
        "total_entities": 456
      }
    }
  ],
  "total_nodes": 1,
  "healthy_nodes": 1,
  "timestamp": "2025-10-20T12:00:00Z"
}
```

---

## Part 14: Data Structures & Value Objects

### 14.1 Core Value Objects

#### EventType
- Format: lowercase with dot notation (e.g., "user.created")
- Validation: no uppercase, no spaces
- Namespace support (e.g., "user" namespace)

#### EntityId
- Non-empty identifier
- Represents aggregate root
- Scoped within tenant

#### TenantId
- Non-empty identifier
- Isolation boundary
- Default tenant support

#### PartitionKey
- Derived from entity ID
- Deterministic partitioning
- Configurable partition count (default 256)

### 14.2 DTOs (Data Transfer Objects)

- EventDto - Event representation
- ProjectionDto - Projection representation
- SchemaDto - Schema representation
- TenantDto - Tenant representation
- All DTOs include serialization/deserialization

---

## Part 15: Configuration & Deployment

### 15.1 EventStore Configuration

```
EventStoreConfig {
  storage_path: String,           // Where to store events
  enable_cache: bool,             // Enable caching layer
  cache_size_mb: u32,             // Cache size limit
  enable_compression: bool,       // Compress stored events
  max_batch_size: usize,          // Max events per batch
  snapshot_interval: usize,       // Auto-snapshot frequency
  enable_ssl: bool,               // TLS support
  ssl_cert_path: Option<String>,
  ssl_key_path: Option<String>
}
```

### 15.2 Port Configuration

- **Core Service**: Port 3900 (Rust Axum server)
- **Control Plane**: Port 3901 (Go Gin server)
- **Query Service**: Port 3902 (Elixir Phoenix server) ✨ NEW
- Configurable via environment variables

---

## Part 16: API Summary

### 16.1 Event Operations

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/api/v1/events` | Ingest single event |
| GET | `/api/v1/events/query` | Query events with filters |
| GET | `/api/v1/events/stream` | WebSocket event stream |

### 16.2 Entity Operations

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/api/v1/entities/{entity_id}/state` | Get current entity state |
| GET | `/api/v1/entities/{entity_id}/snapshot` | Get entity snapshot |

### 16.3 Snapshot Operations

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/api/v1/operations/snapshots` | Create snapshot (Control Plane) |
| GET | `/api/v1/operations/snapshots` | List snapshots (Control Plane) |

### 16.4 Schema Operations

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/api/v1/schemas` | Register schema |
| GET | `/api/v1/schemas` | List subjects |
| GET | `/api/v1/schemas/{subject}` | Get schema |
| GET | `/api/v1/schemas/{subject}/versions` | List versions |
| POST | `/api/v1/schemas/validate` | Validate event |
| PUT | `/api/v1/schemas/{subject}/compatibility` | Set compatibility |

### 16.5 Replay Operations

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/api/v1/replay` | Start replay |
| GET | `/api/v1/replay` | List replays |
| GET | `/api/v1/replay/{replay_id}` | Get progress |
| POST | `/api/v1/replay/{replay_id}/cancel` | Cancel replay |
| DELETE | `/api/v1/replay/{replay_id}` | Delete replay |

### 16.6 Pipeline Operations

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/api/v1/pipelines` | Register pipeline |
| GET | `/api/v1/pipelines` | List pipelines |
| GET | `/api/v1/pipelines/{pipeline_id}` | Get pipeline |
| DELETE | `/api/v1/pipelines/{pipeline_id}` | Delete pipeline |
| GET | `/api/v1/pipelines/{pipeline_id}/stats` | Get stats |
| PUT | `/api/v1/pipelines/{pipeline_id}/reset` | Reset pipeline |
| GET | `/api/v1/pipelines/stats` | All pipeline stats |

### 16.7 Analytics Operations

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/api/v1/analytics/frequency` | Event frequency |
| GET | `/api/v1/analytics/summary` | Stats summary |
| GET | `/api/v1/analytics/correlation` | Correlation analysis |

### 16.8 Compaction Operations

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/api/v1/compaction/trigger` | Trigger compaction |
| GET | `/api/v1/compaction/stats` | Compaction statistics |

### 16.9 System Operations

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/health` | Health check |
| GET | `/metrics` | Prometheus metrics |
| GET | `/api/v1/stats` | Aggregated statistics |

---

## Part 17: Demonstration Scenarios

### Scenario 1: User Registration System
```
1. Ingest event: "user.created" with user details
2. Query events for user entity
3. Create projection "user_snapshots" to maintain current state
4. Get entity state by replaying events
5. Analytics: frequency of user.created events over time
```

### Scenario 2: Order Processing
```
1. Ingest "order.placed" event
2. Query order history with time range
3. Create pipeline to transform order events
4. Snapshot latest order state
5. Replay to fix incorrect projection
6. Correlation analysis to link related events
```

### Scenario 3: Multi-Tenant SaaS
```
1. Create separate tenant per customer
2. Ingest events scoped to tenant
3. Schema registry with tenant-specific subjects
4. Projections per tenant
5. Audit logging of all operations
6. Rate limiting per tenant quota
```

### Scenario 4: Real-time Analytics
```
1. WebSocket subscribe to event stream
2. Window-based aggregation in pipeline
3. Frequency analysis with configurable windows
4. Statistical summary of events
5. Correlation detection
```

---

## Part 18: Key Capabilities Matrix

| Capability | Status | Notes |
|-----------|--------|-------|
| Event Ingestion | ✅ | Single and batch |
| Event Queries | ✅ | Multiple filter types |
| Time-Travel Queries | ✅ | As-of timestamps |
| Projections | ✅ | 5 types with lifecycle |
| Schema Registry | ✅ | Versioning, compatibility |
| Snapshots | ✅ | Manual and automatic |
| Compaction | ✅ | Multiple strategies |
| Replay | ✅ | Partial and full |
| Pipelines | ✅ | 6 operator types |
| Analytics | ✅ | Frequency, stats, correlation |
| Multi-Tenancy | ✅ | Full isolation |
| RBAC | ✅ | 4 roles, 7 permissions |
| JWT Auth | ✅ | Token-based |
| API Keys | ✅ | Per-tenant |
| Audit Logging | ✅ | Immutable logs |
| Encryption | ✅ | At-rest and in-transit |
| Backup | ✅ | Full and incremental |
| Monitoring | ✅ | Prometheus + custom |
| WebSocket | ✅ | Real-time streaming |
| Compression | ✅ | Gzip storage |
| Disaster Recovery | ✅ | Full restore capability |

---

## Part 19: Performance Characteristics

- **Ingestion Throughput**: 469K events/second
- **Query Latency**: Varies by query type (typically <100ms)
- **Compression Ratio**: 60-80% space savings with gzip
- **Concurrent Connections**: Thousands via WebSocket
- **Partition Count**: 256 (configurable)
- **Batch Processing**: Configurable batch sizes

---

## Part 20: Technology Stack

### Core Service
- **Language**: Rust 1.92+
- **Framework**: Axum (async web)
- **Storage**: Parquet files
- **Serialization**: serde_json
- **Async Runtime**: Tokio

### Control Plane
- **Language**: Go 1.22+
- **Framework**: Gin (HTTP router)
- **HTTP Client**: Resty
- **Metrics**: Prometheus client

### Shared Technologies
- UUID generation
- DateTime handling (Chrono for Rust, native for Go)
- JWT (jsonwebtoken)
- Encryption (Argon2, OpenSSL)
- Compression (gzip)
- CORS support

