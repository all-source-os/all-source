# Claude Code Skills for Chronos

This repo ships with Claude Code skills — slash commands that automate testing, releasing, and diagnostics. They live in `.claude/skills/` and are available to anyone using [Claude Code](https://docs.anthropic.com/en/docs/claude-code) in this repo.

## Available Skills

### Testing Skills

| Skill | Trigger | What It Does |
|-------|---------|-------------|
| **chronos-data-flow** | "test data flow", "chronos health" | Tests the full Docker stack (Core, Query Service, MCP) — container reachability, health endpoints, E2E write/read, Query DSL |
| **chronos-data-flow-embedded** | "test embedded", "test nif" | Tests the MCP server with embedded Core backend (`CORE_MODE=embedded`) — NIF prerequisites, ingest/query, schema ops, persistence |
| **chronos-durability** | "test durability", "crash recovery" | Writes events to Core, restarts the container, verifies all events survive — proves WAL + Parquet persistence |
| **chronos-embedded-durability** | "test embedded durability", "issue 84" | Runs Rust integration tests for WAL recovery, crash recovery, and checkpoint correctness — catches the #84 silent-no-op bug class |

### Operations Skills

| Skill | Trigger | What It Does |
|-------|---------|-------------|
| **chronos-release** | "release", "bump version", "tag a release" | Bumps versions across all services, runs CI to green, creates a single squashed commit with an immutable annotated tag |

## Quick Start

### 1. Install Claude Code

```bash
npm install -g @anthropic-ai/claude-code
```

### 2. Run from the repo root

```bash
cd chronos-monorepo
claude
```

Skills are auto-discovered from `.claude/skills/`. No configuration needed.

### 3. Use natural language

```
> test data flow
> test embedded durability
> release 0.14.0
```

Or use the trigger phrases listed in each skill's description.

## Skill Details

### chronos-data-flow

Tests the Docker stack defined in `docker-compose.yml`. Runs `tooling/data-flow-test/test-data-flow.sh`.

```bash
# What it checks
Container State    → Core (3280), Query Service (3283), MCP (3904)
Core Health        → /health, /api/v1/stats, events/query, projections, schemas, snapshots
Query Service      → /api/health, /api/tenant, /api/events, /api/streams, /api/openapi
MCP Server         → /health
E2E Flow           → Create event → read from Core → read via Query Service → verify in streams/types → Query DSL
```

Flags: `--state`, `--core`, `--query`, `--mcp`, `--flow`, `--json`

### chronos-data-flow-embedded

Tests the MCP server running with `CORE_MODE=embedded` (Core via Rustler NIFs, no separate container). Runs `tooling/embedded-data-flow-test/test-embedded-data-flow.sh`.

```bash
# What it checks
Prerequisites      → NIF source/binary, CoreBackend behaviour, CoreEmbedded module, CORE_MODE config
Server Start       → Start MCP with CORE_MODE=embedded, JSON-RPC initialize handshake
Event Ingestion    → Single event + batch (3 events) via ingest_event tool
Event Querying     → By entity_id, event_type, with limit, state reconstruction
Schema Ops         → Register, list, get schema
Stats/Health       → get_stats, storage_stats, wal_status, deep health
Persistence        → Write → stop → restart with same data dir → verify events survived (--persist)
```

Flags: `--prereqs`, `--ingest`, `--query`, `--schema`, `--stats`, `--persist`, `--json`

### chronos-durability

Proves events survive container restarts. Runs `tooling/durability-test/test-durability.sh`.

```bash
# 9-phase test
Pre-flight     → Core reachable, docker CLI available
Baseline       → Current event count
Write          → POST N events to /api/v1/events
Verify Write   → Query back, confirm count
Storage Check  → WAL status, storage stats (informational)
Restart        → docker restart / fly machines restart
Recovery       → Poll /health until back (timeout 60s)
Verify         → Query same events — ALL must survive
Investigation  → Post-restart stats comparison
```

Flags: `--target docker|compose|fly`, `--count N`, `--skip-restart`, `--json`

### chronos-embedded-durability

Runs Rust integration tests that exercise crash recovery code paths directly — no Docker, no HTTP. Runs `tooling/embedded-durability-test/test-embedded-durability.sh`.

Created specifically to catch the [#84 bug class](https://github.com/all-source-os/all-source/issues/84): WAL recovery loads events into memory, `flush_storage()` silently no-ops (empty Parquet batch), WAL is truncated, events lost.

```bash
# Test modes
--quick      → #84-critical tests only (WAL drop-without-flush, Parquet recovery, unclean restart)
--status     → durability_status() API regression tests (5 invariant checks)
--store      → Full EventStore integration tests
--embedded   → Full EmbeddedCore API tests
--chaos      → Chaos/resilience + stress WAL recovery tests
--json       → Machine-readable output
(no flag)    → All of the above
```

The `--status` mode tests `EmbeddedCore::durability_status()`, which returns:

```json
{
  "memory_events": 63,
  "wal_entries": 63,
  "parquet_files": 0,
  "parquet_pending_batch": 63,
  "durable": true,
  "warnings": []
}
```

Key invariant: after WAL recovery, `parquet_pending_batch` must equal `memory_events` before WAL truncation. If it's 0, the #84 bug is present.

### chronos-release

Cuts a versioned release. See also `docs/guides/RELEASE.md` for the `make release` workflow.

```bash
# Usage
> release 0.14.0        # explicit version
> release patch          # auto-increment patch
> release minor          # auto-increment minor
```

Procedure: determine version → check preconditions (clean tree, main branch, tag doesn't exist) → `make set-version` → `make ci` → fix any failures → single squashed commit → annotated tag → report (does NOT push).

## Test Coverage Map

Which skill catches which failure mode:

| Failure Mode | data-flow | data-flow-embedded | durability | embedded-durability |
|---|:---:|:---:|:---:|:---:|
| Core container down | X | | | |
| Query Service routing broken | X | | | |
| MCP health endpoint down | X | X | | |
| NIF not compiled | | X | | |
| Events lost on Docker restart | | | X | |
| #84: silent no-op checkpoint | | | | X |
| WAL truncation before Parquet write | | | | X |
| Unclean shutdown data loss | | | partially | X |
| Embedded backend ingest/query | | X | | |
| Schema operations broken | | X | | |
| WAL fsync not working | | | | X |

## File Layout

```
.claude/
└── skills/
    ├── chronos-data-flow/SKILL.md
    ├── chronos-data-flow-embedded/SKILL.md
    ├── chronos-durability/SKILL.md
    ├── chronos-embedded-durability/SKILL.md
    └── chronos-release/SKILL.md

tooling/
├── data-flow-test/test-data-flow.sh
├── durability-test/test-durability.sh
├── embedded-data-flow-test/test-embedded-data-flow.sh
└── embedded-durability-test/test-embedded-durability.sh
```

Skills are SKILL.md files that tell Claude Code what to do. The actual test logic lives in the shell scripts under `tooling/`. You can run the scripts directly without Claude Code:

```bash
bash tooling/data-flow-test/test-data-flow.sh
bash tooling/durability-test/test-durability.sh --target docker
bash tooling/embedded-durability-test/test-embedded-durability.sh --quick
```

## Creating New Skills

Skills are markdown files in `.claude/skills/<name>/SKILL.md` with YAML frontmatter:

```yaml
---
name: my-skill
description: What it does and trigger phrases.
category: testing
color: green
displayName: My Skill
---
```

The body describes the procedure Claude Code should follow when the skill is triggered. Skills can reference shell scripts, cargo tests, or any CLI tooling.

See the [Claude Code docs on skills](https://docs.anthropic.com/en/docs/claude-code/skills) for the full spec.

## Adopting These Skills in Your Fork

If you fork the chronos monorepo:

1. Skills come with the repo — `.claude/skills/` is checked into git
2. Test scripts come with the repo — `tooling/` is checked into git
3. Install Claude Code and run `claude` from the repo root
4. Skills are available immediately — no setup needed

If you want to adapt skills for a different project:

1. Copy the `.claude/skills/<name>/SKILL.md` file
2. Copy the corresponding `tooling/<name>/test-*.sh` script
3. Update ports, URLs, and service names to match your stack
4. Update the SKILL.md trigger phrases and test descriptions

The skills are self-contained — each SKILL.md describes its own procedure, and each shell script is standalone with no external dependencies beyond `curl`, `bash`, and standard CLI tools (`cargo`, `mix`, `docker`, `gh`).
