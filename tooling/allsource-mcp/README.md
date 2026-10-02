# allsource-mcp

Lightweight MCP server for local AllSource debugging. Reads directly from WAL + Parquet files on disk — no running Core server needed.

## Install

```bash
cargo install allsource-mcp
```

Or build from source:

```bash
git clone https://github.com/all-source-os/all-source.git
cd all-source/tooling/allsource-mcp
cargo install --path .
```

## Claude Code Configuration

Add to `~/.claude/settings.json` (or project `.claude/settings.json`):

```json
{
  "mcpServers": {
    "allsource": {
      "command": "allsource-mcp",
      "args": [
        "--data-dir", "/path/to/allsource/data",
        "--profile", "hosted-tenant",
        "--tenant-id", "tenant-123",
        "--source-id", "production-eu"
      ],
      "env": {}
    }
  }
}
```

`hosted-tenant` fails closed without an immutable `--tenant-id`. Request arguments cannot override this binding. `local` remains default and reports an unbound store as unverified. `operator` is an explicit broad-access profile.

For Longhand on macOS:

```json
{
  "mcpServers": {
    "allsource": {
      "command": "allsource-mcp",
      "args": ["--data-dir", "~/Library/Application Support/Longhand/allsource"],
      "env": {}
    }
  }
}
```

Or use the environment variable instead of `--data-dir`:

```json
{
  "mcpServers": {
    "allsource": {
      "command": "allsource-mcp",
      "env": {
        "ALLSOURCE_DATA_DIR": "~/Library/Application Support/Longhand/allsource"
      }
    }
  }
}
```

## Available Tools

| Tool | Description |
|------|-------------|
| `query_events` | Tenant-bound paginated events with completeness metadata |
| `sample_events` | Recent events inside the configured tenant boundary |
| `quick_stats` | Exact scoped counts, freshness, and durability |
| `get_snapshot` | Named authoritative projection state; no guessed fallback |
| `event_timeline` | Paginated chronological entity timeline |
| `explain_entity` | Human-readable lifecycle summary of an entity |
| `reconstruct_state` | Deprecated, explicitly non-authoritative payload-fold preview |
| `analyze_changes` | Paginated changes within a strict RFC 3339 window |
| `list_stores` | Every configured store with its path, event count and last refresh |
| `watch_events` | Long-poll for events newer than a checkpoint |
| `fold_entity_lifecycle` | Latest state per entity for one event family |
| `fold_steps` | Start/terminal pairs per payload key, with elapsed time |
| `trace` | Follow one id across every store: events that reference it, then the ids they carry, up to `depth` hops |

Every successful result includes JSON `structuredContent`, tenant/source provenance, freshness, and completeness. Existing pretty-JSON text content remains for older clients.

## Freshness

The server opens each store read-only and catches it up with the store's writer before a read: new Parquet files are loaded and the WAL is re-read when a segment changed. It never reopens the data dir and never writes to it. `--refresh-ms` (env `ALLSOURCE_MCP_REFRESH_MS`, default `1000`) is the minimum gap between two refreshes of one store; `0` refreshes before every read. Each result carries `context.store.refreshedAt` and `context.store.newEventsOnLastRefresh` (`context.stores.<name>` for `trace`), so an old `freshThrough` can be told apart from a reader that stopped catching up.

## trace

`trace` scans each requested store once (bounded by `max_scan` per store) and walks the id graph in memory, so a deeper trace does not re-read a store. An event joins at hop *n* when its entity id equals an id found at hop *n − 1*, or its payload, in the requested `payload_mode`, contains one. The ids an event carries are its entity id plus string values under `id_keys` (default `id`, `_id`, `Id`, `_ids`, `_ref`, `Ref`), read from that same view, so a redacted value is never matched or followed. A string holding a JSON object or array (a tool result stored as text) is parsed before that view is taken, so ids inside it join and its credential keys are redacted. A carried id whose matching events span more than `hub_threshold` distinct entities (default 20, such as a workspace or workflow id every run shares) is listed under `graph.hubs` and not followed. Each item adds `store`, `hop`, `matched_by` and `carries`; `graph` lists the ids and the edges between them; `completeness.reason` is `max_scan_reached`, `frontier_truncated` (more than 50 new ids at one hop) or `limit_reached`. A hosted tenant reads only `default`.

## Example Session

```
> Use query_events to find all workflow_run events for entity workflow:abc-123

Found 5 events:
1. workflow_run.started (2024-01-15T10:00:00Z)
2. workflow_run.step_completed (2024-01-15T10:00:05Z)
3. workflow_run.step_completed (2024-01-15T10:00:12Z)
4. workflow_run.step_completed (2024-01-15T10:00:18Z)
5. workflow_run.completed (2024-01-15T10:00:20Z)

> Use explain_entity to summarize this workflow

Entity workflow:abc-123 has 5 events spanning 20 seconds.
Lifecycle: started → 3 steps completed → completed
```
