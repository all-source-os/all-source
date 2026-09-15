# Projection Debugging Guide

How to verify projection state matches raw events, detect replay gaps, and fix stale or corrupt projections.

## Key Concept

Projections are **derived views** computed from the event stream. They are always rebuildable — if a projection is wrong, the events are the source of truth.

```
Events (immutable) ──► Projection (derived, rebuildable)
```

## Common Issues

### 1. Projection shows wrong count

**Symptom**: `runsCount: 0` but you know there are runs.

**Diagnosis**:

```bash
# Step 1: Query raw events for the entity
allsource-inspect events --data-dir <path> --limit 100000 --entity-id "workflow:abc-123" --format json

# Step 2: Check event types
allsource-inspect events --data-dir <path> --limit 100000 --entity-id "workflow:abc-123" --format json \
  | jq '.event_type' | sort | uniq -c

# Step 3: Compare with projection state
# (via allsource-mcp) Use get_snapshot for entity "workflow:abc-123"
# (via allsource-mcp) Use reconstruct_state for entity "workflow:abc-123"
# Compare the two outputs
```

**Common causes**:
- Event missing a field the projection indexes on (e.g., `definition_id` not set)
- Projection filters on event type that doesn't match (e.g., `run.started` vs `workflow_run.started`)
- Entity ID format mismatch (e.g., `workflow:abc` vs `workflow:abc-123`)

### 2. Stale projection (old data)

**Symptom**: Projection shows data from hours/days ago, new events not reflected.

**Diagnosis**:

```bash
# Check latest event timestamp
allsource-inspect events --data-dir <path> --limit 100000 --entity-id "<entity>" --format json \
  | jq -r '.timestamp' | tail -1

# Check durability status
# (via allsource-mcp) Use quick_stats — check wal_entries and parquet state
```

**Common causes**:
- Events in WAL but not yet flushed to Parquet (check `wal_entries > 0`)
- Projection registered after events were ingested (missed initial replay)
- Application restarted but projection not re-registered

### 3. Missing events in replay

**Symptom**: Event count doesn't match expected, gaps in sequence.

**Diagnosis**:

```bash
# Get event timeline with version numbers
# (via allsource-mcp) Use event_timeline for entity "<entity>"

# Check for gaps: versions should be monotonically increasing
# Gap between version 5 and version 8 means events 6, 7 were lost or misrouted
```

**Common causes**:
- Crash between DashMap insert and WAL write (rare with fsync)
- WAL corruption (CRC checksum failure on recovery)
- Events ingested with wrong entity_id (check other entity IDs)

### 4. Duplicate events

**Symptom**: Count is too high, same event appears twice.

**Diagnosis**:

```bash
# Check for duplicate event IDs
allsource-inspect events --data-dir <path> --limit 100000 --entity-id "<entity>" --format json \
  | jq -r '.id' | sort | uniq -d

# Check for duplicate timestamps + event types
allsource-inspect events --data-dir <path> --limit 100000 --entity-id "<entity>" --format json \
  | jq -r '[.timestamp, .event_type] | join(" ")' | sort | uniq -d
```

**Common causes**:
- Application retrying ingest without idempotency check
- WAL replay after Parquet already had the event (dedup should handle this)

## Worked Example: Longhand Workflow Projection Bug

**Problem**: `WorkflowDefinition` projection showed `runsCount: 0` for a definition with 5 actual runs.

**Investigation**:

```bash
# 1. Find all run events
allsource-inspect events \
  --data-dir <app-data-dir>/allsource \
  --event-type-prefix "workflow_run." \
  --format json

# Found 5 workflow_run.started events, but entity_id was
# "execution_workflow:xxx" not "workflow_definition:yyy"

# 2. Check what the projection was looking for
# The projection indexed by definition_id field in the event payload

# 3. Check if definition_id was set in the events
cat events.json | jq '.payload.definition_id'
# Result: null — the field was not being set at ingest time
```

**Root cause**: `WorkflowRunEvent::Started` was not populating `definition_id` in the payload. The projection had no way to associate runs with definitions.

**Fix**: Added `definition_id` to the `Started` event payload and a `by_definition` secondary index to the projection.

## Verification Checklist

After fixing a projection issue:

- [ ] Query raw events — do they have the expected fields?
- [ ] Reconstruct state from events — does the folded state match expectations?
- [ ] Check projection state — does it match the reconstructed state?
- [ ] Verify event count — do you have the right number of events?
- [ ] Check timestamps — are events in the expected time range?
- [ ] Test with fresh projection rebuild — does a clean rebuild produce correct results?

## Tools Reference

| Need | Tool |
|------|------|
| Find which data directory to read | `allsource-inspect stores --root <dir>` |
| See all events for an entity | `query_events` / `allsource-inspect events --entity-id` |
| See event type distribution | `quick_stats` / `allsource-inspect summary` |
| Compare projection vs events | `get_snapshot` + `reconstruct_state` |
| Trace entity lifecycle | `event_timeline` / `explain_entity` |
| Current state of many entities at once | `allsource-inspect lifecycle` |
| Find changes in time window | `analyze_changes` |
| Check store health | `quick_stats` (durability section) |
| Read uncommitted WAL events | `allsource-inspect wal` |
