# AllSource Data Access

Understand AllSource storage internals: WAL format, Parquet schema, event flow, and how to read data files without a running server.

Triggers on: "read parquet", "read wal", "storage layout", "event schema", "allsource data files", "where are events stored", "parquet schema".

---

## Storage Layout

```
<data_dir>/
├── storage/                    # Parquet files (long-term columnar storage)
│   ├── events-20240115-001.parquet
│   ├── events-20240115-002.parquet
│   └── events-20240116-001.parquet
├── wal/                        # Write-Ahead Log (crash recovery)
│   ├── wal-0000000000000000.log
│   └── wal-0000000000000001.log
└── __system/                   # System metadata (tenants, config, auth)
    └── wal/
        └── wal-0000000000000000.log
```

### Default Data Paths

| Platform | App | Path |
|----------|-----|------|
| macOS | Longhand | `~/Library/Application Support/Longhand/allsource/` |
| Linux | Longhand | `~/.local/share/longhand/allsource/` |
| Docker Core | Mounted | `/app/data/` (via volume) |
| Embedded (default) | In-process | `data/mcp-embedded/` or configured via `CORE_DATA_DIR` |

---

## Parquet Schema

Each Parquet file contains events with this Arrow schema:

| Column | Arrow Type | Nullable | Description |
|--------|-----------|----------|-------------|
| `event_id` | `Utf8` | No | UUID v4 string |
| `event_type` | `Utf8` | No | Dot-separated type (e.g., `workflow_run.started`) |
| `entity_id` | `Utf8` | No | Entity identifier (e.g., `workflow:abc-123`) |
| `payload` | `Utf8` | No | JSON string of event data |
| `timestamp` | `Timestamp(Microsecond)` | No | Event creation time |
| `metadata` | `Utf8` | Yes | Optional JSON string (correlation IDs, source info) |

### Parquet Configuration

- **Compression**: Snappy (default)
- **Batch size**: 10,000 events per flush (configurable)
- **Flush timeout**: 5 seconds (partial batch written if timeout reached)
- **File naming**: `events-YYYYMMDD-NNN.parquet` (date-partitioned, sequential)
- **High-throughput mode**: 50,000 events/batch, 10-second timeout

### Reading Parquet Files

With `allsource-inspect` (read-only, safe beside a running app):
```bash
allsource-inspect events --data-dir <path> --format json
allsource-inspect events --data-dir <path> --event-type-prefix "workflow_run." --fields definition_id --format json
```

With Python (if pyarrow available):
```python
import pyarrow.parquet as pq
table = pq.read_table("storage/events-20240115-001.parquet")
df = table.to_pandas()
print(df[df.entity_id == "workflow:abc-123"])
```

With the allsource-mcp MCP server (no extra deps):
```
Use quick_stats to see total events and parquet file count
Use query_events with entity_id to read specific events
```

---

## WAL Format

The WAL uses **JSON Lines** format — one JSON object per line, each representing a `WALEntry`:

```json
{"sequence":1,"wal_timestamp":"2024-01-15T10:00:00Z","event":{...},"checksum":3847291654}
```

### WAL Entry Structure

| Field | Type | Description |
|-------|------|-------------|
| `sequence` | `u64` | Monotonically increasing sequence number |
| `wal_timestamp` | `DateTime<Utc>` | When the entry was written to WAL |
| `event` | `Event` | The full event object (same fields as Parquet) |
| `checksum` | `u32` | CRC32 checksum of `sequence + wal_timestamp + event.id` |

### WAL Configuration

| Setting | Default | Description |
|---------|---------|-------------|
| `max_file_size` | 64 MB | Rotate WAL file after this size |
| `sync_on_write` | `true` | fsync after each write (durability vs throughput) |
| `max_wal_files` | 10 | Max WAL files before cleanup |
| `compress` | `false` | LZ4 compression (optional) |

### WAL File Naming

Files are named `wal-NNNNNNNNNNNNNNNN.log` where N is a zero-padded sequence number.

### Reading WAL Files

WAL files are plain text, one JSON object per line:
```bash
# Read WAL entries as JSON
cat wal/wal-0000000000000000.log | head -5

# Count entries
wc -l wal/wal-0000000000000000.log

# Filter by entity
grep '"entity_id":"workflow:abc-123"' wal/wal-0000000000000000.log
```

With `allsource-inspect`:
```bash
allsource-inspect wal --data-dir <path> --format json
```

### Verifying WAL Integrity

Each entry has a CRC32 checksum. Replay verifies it and skips a mismatched entry
with a `Corrupted WAL entry … (checksum mismatch)` warning, so run any read
command with tracing on and look for that line. No CLI flag checks it on demand.

---

## Event Flow

```
Ingest API/Embedded
       │
       ▼
   DashMap (in-memory concurrent map)
       │  ← 11.9μs query latency, 469K events/sec
       │
       ├──▶ WAL (append-only, fsync)
       │       │  ← crash recovery source
       │       │
       │       ▼
       │    Parquet (columnar, Snappy)
       │       ← long-term storage, efficient reads
       │
       ▼
   Query (reads from DashMap + Parquet)
```

### Write Path
1. Event hits `DashMap` (immediate availability for queries)
2. Event appended to WAL (durability guarantee)
3. When batch reaches 10,000 or 5-second timeout → flush to Parquet
4. WAL entries for flushed events can be cleaned up

### Read Path
1. Query reads from in-memory `DashMap` first
2. Parquet files provide historical data
3. Combined result returned to caller

### Recovery Path (on startup)
1. Read existing Parquet files → populate DashMap
2. Replay WAL entries not yet in Parquet → recover unflushed events
3. System is consistent — no data loss

---

## System Events

System metadata uses a separate WAL in `__system/wal/`:

| Domain | Event Types | Purpose |
|--------|------------|---------|
| `_system.tenant.*` | created, updated, suspended | Tenant lifecycle |
| `_system.config.*` | set, deleted | Key-value configuration |
| `_system.auth.*` | key_provisioned, key_revoked | API key management |
| `_system.schema.*` | registered, updated, deprecated | Event schema registry |
| `_system.policy.*` | created, updated, deleted | Access policies |
| `_system.consumer.*` | registered, ack_updated, deleted | Subscription cursors |

System events use the same WAL format but are stored in `__system/` to separate metadata from user event data.

---

## Reading Without a Running Server

The allsource-mcp and allsource-inspect tools open the data directory directly using the `allsource-core` embedded API. No server process needed.

```rust
use allsource_core::embedded::{Config, EmbeddedCore, Query};

let core = EmbeddedCore::open(
    Config::builder()
        .data_dir("/path/to/data")
        .read_only(true)
        .build()?
).await?;

let events = core.query(Query::new().entity_id("workflow:abc-123")).await?;
// Do not call core.shutdown() on a read-only core: it flushes storage.
```

This opens Parquet files and replays the WAL for reads. **`read_only(true)` is
required when anything else may own the directory.** Without it the open
checkpoints the recovered WAL into Parquet and truncates the WAL out from under
the owning process, losing the writes it is still appending (#201).
