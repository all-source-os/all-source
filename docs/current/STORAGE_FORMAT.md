# AllSource Core Storage Format

How events flow from ingest to durable storage, and how to read the raw files.

## Storage Architecture

```
Ingest (API or Embedded)
       │
       ▼
   DashMap (in-memory concurrent hash map)
       │  ← immediate query availability
       │
       ├──► WAL (Write-Ahead Log)
       │       ← crash recovery, sequential writes
       │
       └──► Parquet (columnar storage)
               ← long-term persistence, efficient scans
```

## Directory Layout

```
<data_dir>/
├── storage/                         # Parquet files
│   ├── events-20240115-001.parquet  # Date-partitioned
│   ├── events-20240115-002.parquet  # Sequential within date
│   └── events-20240116-001.parquet
├── wal/                             # Write-Ahead Log
│   ├── wal-0000000000000000.log     # Active WAL
│   └── wal-0000000000000001.log     # Rotated WAL
└── __system/                        # System metadata
    └── wal/                         # Tenants, config, auth, schemas
        └── wal-0000000000000000.log
```

## Parquet Schema

Each Parquet file uses this Arrow schema:

| Column | Type | Nullable | Description |
|--------|------|----------|-------------|
| `event_id` | `Utf8` | No | UUID v4 identifier |
| `event_type` | `Utf8` | No | Dot-separated type (e.g., `workflow_run.started`) |
| `entity_id` | `Utf8` | No | Entity identifier (e.g., `workflow:abc-123`) |
| `payload` | `Utf8` | No | JSON-serialized event data |
| `timestamp` | `Timestamp(Microsecond)` | No | Event creation time (UTC) |
| `metadata` | `Utf8` | Yes | Optional JSON (correlation IDs, source) |

### Parquet Configuration

| Setting | Default | High-throughput |
|---------|---------|-----------------|
| Compression | Snappy | Snappy |
| Batch size | 10,000 events | 50,000 events |
| Flush timeout | 5 seconds | 10 seconds |
| File naming | `events-YYYYMMDD-NNN.parquet` | same |

Events accumulate in a batch buffer. Flush triggers when:
1. Batch reaches `batch_size` events, OR
2. `flush_timeout` elapses since last flush, OR
3. Shutdown is initiated (flush remaining batch)

## WAL Format

The WAL uses **JSON Lines** (one JSON object per line):

```json
{"sequence":1,"wal_timestamp":"2024-01-15T10:00:00.123456Z","event":{"id":"550e8400-...","event_type":"order.placed","entity_id":"order:123","payload":{"amount":99.99},"timestamp":"2024-01-15T10:00:00Z","metadata":null,"tenant_id":"default","version":1},"checksum":3847291654}
```

### WAL Entry Fields

| Field | Type | Description |
|-------|------|-------------|
| `sequence` | `u64` | Monotonically increasing per WAL file |
| `wal_timestamp` | `DateTime<Utc>` | When written to WAL (not event timestamp) |
| `event` | `Event` | Full event object |
| `checksum` | `u32` | CRC32 of `"{sequence}{wal_timestamp}{event.id}"` |

### WAL Configuration

| Setting | Default | Description |
|---------|---------|-------------|
| `max_file_size` | 64 MB | Rotate to new file at this size |
| `sync_on_write` | `true` | `fsync()` after each write |
| `max_wal_files` | 10 | Old files cleaned after this count |
| `compress` | `false` | Optional LZ4 compression |

### WAL Integrity

Each entry carries a CRC32 checksum. On recovery:
- Entries with valid checksums are replayed
- Entries with invalid checksums indicate partial writes (crash mid-write) and are skipped
- The last entry in a WAL file may be partial — this is normal and expected after a crash

## Event Flow Detail

### Write Path

1. **Ingest** → event assigned UUID, timestamp, version
2. **DashMap insert** → event immediately queryable
3. **WAL append** → JSON line written, optionally fsynced
4. **Batch buffer** → event added to pending Parquet batch
5. **Parquet flush** → when batch full or timeout, written as Parquet file

### Read Path

1. **Query** → reads from DashMap (in-memory, ~11.9μs latency)
2. **Historical** → Parquet files scanned for matching events
3. **Combined** → both sources merged, deduplicated by event ID

### Startup Recovery

1. Scan `storage/` → read all Parquet files into DashMap
2. Scan `wal/` → replay WAL entries not already in DashMap
3. Verify checksums → skip corrupt entries
4. Resume normal operation

## System Metadata Store

The `__system/` directory uses the same WAL format but stores operational metadata:

| Domain | Entity ID Pattern | Events |
|--------|------------------|--------|
| Tenant | `_system:tenant:<id>` | created, updated, suspended |
| Config | `_system:config:<key>` | set, deleted |
| Auth | `_system:auth:<key-id>` | key_provisioned, key_revoked |
| Schema | `_system:schema:<name>` | registered, updated, deprecated |
| Policy | `_system:policy:<id>` | created, updated, deleted |
| Consumer | `_system:consumer:<id>` | registered, ack_updated, deleted |

System metadata does not use Parquet — it's WAL-only with in-memory replay on startup. This is appropriate because system metadata is small (dozens to hundreds of entries) and changes infrequently.

## Reading Files Without a Server

### Using allsource-inspect CLI

Every command opens the store **read-only**: the WAL is replayed for reads and
never truncated, so it is safe beside a running application.

```bash
# Which data directories exist under an app's support dir (opens none of them)
allsource-inspect stores --root <app-support-dir>

# All events for an entity
allsource-inspect events --data-dir <path> --entity-id "workflow:abc-123"

# Events by type prefix, payload narrowed to two fields
allsource-inspect events --data-dir <path> --event-type-prefix "workflow_run." \
  --fields definition_id,step.index --format json

# Events whose payload mentions an id
allsource-inspect events --data-dir <path> --contains "abc-123" --keys --format json

# Current state of every run: newest state-setting event per entity
allsource-inspect lifecycle --data-dir <path> --event-type-prefix "workflow_run." \
  --states started,completed,failed --state started --format json

# Storage summary
allsource-inspect summary --data-dir <path>

# Events still in the WAL (not yet flushed to Parquet)
allsource-inspect wal --data-dir <path> --format json
```

### Using allsource-mcp (Claude Code)

Configure the MCP server, then ask Claude to use `quick_stats` or `query_events`.

### Using Python

```python
import pyarrow.parquet as pq

table = pq.read_table("storage/events-20240115-001.parquet")
df = table.to_pandas()

# Filter by entity
entity_events = df[df.entity_id == "workflow:abc-123"]

# Parse JSON payloads
import json
entity_events["parsed"] = entity_events.payload.apply(json.loads)
```

### Reading WAL directly

```bash
# WAL is plain JSON lines
cat wal/wal-0000000000000000.log | python3 -m json.tool --no-ensure-ascii

# Count events
wc -l wal/*.log

# Find events for an entity
grep '"entity_id":"order:123"' wal/*.log | python3 -m json.tool
```
