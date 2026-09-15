# AllSource Debug

Debug AllSource-backed applications by inspecting projections, querying events, tracing replay, and diagnosing stale or corrupt state.

Triggers on: "debug allsource", "inspect events", "why is projection wrong", "stale state", "missing events", "projection mismatch", "event replay", "debug entity".

---

## Tools Available

### If allsource-mcp is configured (preferred)

Use MCP tools directly — they read from the data directory without a running server:

| Tool | Use When |
|------|----------|
| `query_events` | Find events for an entity or event type |
| `quick_stats` | Get store overview (counts, types, date range) |
| `event_timeline` | See chronological event sequence for an entity |
| `explain_entity` | Get human-readable lifecycle summary |
| `reconstruct_state` | Fold events to rebuild current state |
| `get_snapshot` | Get projection/snapshot state |
| `analyze_changes` | Compare state between two time points |
| `sample_events` | Discover what event types exist |

### If allsource-inspect CLI is available

```bash
# Which data directory? An app can keep several (per profile, per workspace)
allsource-inspect stores --root <app-support-dir>

# Find all events for an entity
allsource-inspect events --data-dir <path> --entity-id "workflow:abc-123" --format json

# Find events by type prefix, printing only the payload keys
allsource-inspect events --data-dir <path> --event-type-prefix "workflow_run." --keys --format json

# Current state of every run in one pass
allsource-inspect lifecycle --data-dir <path> --event-type-prefix "workflow_run." \
  --states started,completed,failed --format json

# Storage summary
allsource-inspect summary --data-dir <path>

# Events still in the WAL
allsource-inspect wal --data-dir <path> --format json
```

Every command opens the store read-only, so it is safe to run while the app is writing.

### Default data directory locations

| Platform | App | Path |
|----------|-----|------|
| macOS | Longhand | `~/Library/Application Support/Longhand/allsource/` |
| Linux | Longhand | `~/.local/share/longhand/allsource/` |
| Docker | Core | `/app/data/` (mapped via volume) |

---

## Debugging Workflows

### 1. Projection shows wrong count or missing data

**Symptom**: UI shows `runsCount: 0` but you know there are runs.

**Steps**:
1. Query raw events for the entity:
   ```
   Use query_events with entity_id="<entity>" to see all events
   ```
2. Check if the events have the fields the projection expects:
   ```
   Use event_timeline with entity_id="<entity>" to see event sequence
   ```
3. Compare projection state vs reconstructed state:
   ```
   Use get_snapshot for the projection view
   Use reconstruct_state to fold events manually
   Compare the two — differences indicate projection bugs
   ```

**Common causes**:
- Event missing a field the projection indexes on (e.g., `definition_id`)
- Projection filter doesn't match event type pattern
- Events ingested with wrong entity_id format

### 2. Stale projection state

**Symptom**: Projection shows old data, new events aren't reflected.

**Steps**:
1. Check if events are actually persisted:
   ```
   Use quick_stats to see total event count and latest timestamp
   ```
2. Check WAL vs Parquet:
   ```
   Use quick_stats — if wal_entries > 0 and parquet is behind, events may be in WAL only
   ```
3. Verify the entity has recent events:
   ```
   Use query_events with entity_id and since=<recent timestamp>
   ```

**Common causes**:
- Events in WAL but not yet flushed to Parquet
- Projection registered after events were ingested (missed replay)
- Application crash before WAL fsync

### 3. Duplicate or missing events in replay

**Symptom**: Event count doesn't match expected, or state has duplicates.

**Steps**:
1. Get full event timeline:
   ```
   Use event_timeline with entity_id — check for gaps in version numbers
   ```
2. Look for duplicates:
   ```
   Use query_events with entity_id — check for duplicate timestamps or event types
   ```
3. Check version sequence:
   - Versions should be monotonically increasing per entity
   - Gaps indicate missed events
   - Duplicate versions indicate replay issues

### 4. Understanding an entity's lifecycle

**Steps**:
1. Start with explain_entity for the overview
2. Use event_timeline for the detailed sequence
3. Use analyze_changes with time bounds to focus on a specific period
4. Use reconstruct_state to see the final folded state

### 5. Verifying data after restart

**Steps**:
1. Use quick_stats to check durability status:
   - `durable: true` means all events are persisted
   - `wal_enabled: true` with entries means WAL has data
   - `parquet_files > 0` means long-term storage is working
2. Query a known entity to verify data survived:
   ```
   Use query_events with a known entity_id
   ```

---

## Key Concepts

- **Events are immutable** — once ingested, they cannot be modified or deleted
- **Projections are derived** — they're computed from events and can be rebuilt
- **WAL is the write path** — events hit WAL first, then flush to Parquet
- **Parquet is the read path** — queries read from Parquet + in-memory DashMap
- **Entity ID format** — typically `type:uuid` (e.g., `workflow:abc-123`)
- **Event type format** — dot-separated (e.g., `workflow_run.started`, `workflow_run.completed`)
- **System events** — prefixed with `_system.` (tenants, config, auth, schemas)
