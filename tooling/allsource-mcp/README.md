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

Every successful result includes JSON `structuredContent`, tenant/source provenance, freshness, and completeness. Existing pretty-JSON text content remains for older clients.

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
