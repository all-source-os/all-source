# Chronis Best Practices

## Git Ignore for `.chronis/`

Chronis stores data in a `.chronis/` directory at your project root. Sync is HTTP-based (not git-based), so the gitignore is straightforward: ignore all local data, track only config.

### Recommended `.gitignore` rules

```gitignore
# Chronis (all data is local or synced via HTTP — only config needs tracking)
.chronis/wal/
.chronis/storage/
.chronis/sync/
```

### What each path contains

| Path | Contents | Git? | Why |
|------|----------|------|-----|
| `.chronis/wal/` | Write-ahead log segments (binary, CRC32 checksums) | **Ignore** | Machine-local binary data. Differs per machine. |
| `.chronis/storage/` | Parquet columnar files (binary, Snappy compressed) | **Ignore** | Machine-local binary data. Rebuilt from WAL on startup. |
| `.chronis/sync/` | Sync state (timestamps, dedup cursors) | **Ignore** | Machine-local sync cursor. Each machine tracks its own position. |
| `.chronis/config.toml` | Workspace configuration (mode, instance_id, remote_url) | **Track** | Shared settings should be consistent across team. |

### Key principle

**Ignore local data, track config.** WAL and Parquet are binary formats that vary per machine. Sync state is per-machine. Only `config.toml` is shared.

### Setting up a new project

```bash
cn init                       # Creates .chronis/ directory with internal .gitignore
```

`cn init` creates a `.chronis/.gitignore` inside the workspace. For team projects, also add the rules above to your project's root `.gitignore` so they're explicit and visible to all contributors.

### Common mistakes

- **Ignoring all of `.chronis/`** — hides `config.toml` from git, so team members won't share sync configuration.
- **Tracking `.chronis/wal/` or `.chronis/storage/`** — bloats your repo with binary files that change on every write. These can be hundreds of MB on active projects.

---

## Sync Modes

Chronis supports two modes, configured in `.chronis/config.toml`:

### Embedded mode (default)

Each machine has its own local event store. `cn sync` exchanges events with a remote Core over HTTP. Sync is idempotent — UUID tracking prevents duplicates even when the overlap buffer re-fetches events.

```toml
mode = "embedded"
instance_id = "auto-generated"

[sync]
remote_url = "https://api.all-source.xyz"
```

**When to use:** Local-first development, offline capability, low-latency reads.

### Remote mode

All commands talk directly to a shared remote Core. No local data, no sync step needed.

```toml
mode = "remote"

[sync]
remote_url = "https://api.all-source.xyz"
```

**When to use:** Shared team instance, CI/CD pipelines, environments where you want a single source of truth without sync. Reads use a local in-memory projection cache bootstrapped from remote events on startup.

### Credentials for the hosted gateway

Both modes need a credential when `remote_url` is `https://api.all-source.xyz`. Prefer a registered client key: put `client_id`, `tenant_id`, `client_kid` and `signing_key_path` in `[sync]`, or set `ALLSOURCE_CLIENT_SIGNING_KEY`, `ALLSOURCE_CLIENT_ID`, `ALLSOURCE_TENANT_ID` and `ALLSOURCE_CLIENT_KID` per machine. Keep the private key out of the repository: `config.toml` is tracked, so it carries only the path. An `api_key` in `[sync]` still works; keys from the old onboarding flow stop at the gateway's legacy key sunset. Details: the README's "Authenticating to the hosted gateway" and [`docs/runbooks/CLIENT_KEY_ROTATION.md`](../../../docs/runbooks/CLIENT_KEY_ROTATION.md).

---

## When to Use TOON (and When Not To)

TOON (Token-Oriented Object Notation) is chronis's agent-optimized output format, activated with `--toon` on any command. It cuts token consumption ~50% vs human-readable output.

### Use TOON when

**AI agents are consuming the output.** This is TOON's primary purpose.

```bash
# Agent workflow — every command uses --toon
export CN_AGENT_ID=agent-1
cn ready --toon              # [id|type|title|pri|status|claimed|blocked_by|parent|archived]
cn claim t-abc1 --toon       # ok:claimed:t-abc1
cn done t-abc1 --toon        # ok:done:t-abc1
```

Specific scenarios where TOON is the right choice:

| Scenario | Why TOON |
|----------|----------|
| MCP tool calls | LLM reads the output — every saved token reduces cost and latency |
| Agent orchestration scripts | Agents parsing task lists don't need box-drawing characters |
| CI/CD pipelines | Machine-parseable, grep-friendly, no ANSI escape codes |
| Piping to other tools | Predictable pipe-delimited format, easy to `cut`, `awk`, or parse |
| Large task lists (50+ tasks) | Token savings compound — 50% of 1000 tokens is 500 tokens saved |
| Repeated polling (`cn ready` in a loop) | Each iteration saves tokens; over hundreds of iterations, it adds up |

### Do NOT use TOON when

**A human is reading the output.** TOON sacrifices readability for compactness.

```bash
# Human workflow — default output (no --toon)
cn list                      # Pretty ASCII table with alignment and borders
cn show t-abc1               # Formatted detail view with timeline
cn ready                     # Scannable table of actionable tasks
```

Specific scenarios where human-readable output is better:

| Scenario | Why not TOON |
|----------|-------------|
| Interactive terminal use | Humans read faster with aligned columns and visual separators |
| Debugging task state | `cn show <id>` in human mode shows a formatted timeline that's easier to scan |
| Demos and screenshots | ASCII tables look better in docs, presentations, and tweets |
| Onboarding new team members | Human output is self-documenting — column headers are visible in context |
| TUI mode (`cn tui`) | The TUI has its own rendering — `--toon` doesn't apply |
| Web viewer (`cn serve`) | HTML rendering — `--toon` doesn't apply |

### Decision rule

> **If the consumer is an LLM or a script, use `--toon`. If the consumer is a human eyeball, don't.**

### Setting TOON as default for agents

For agent environments, set the flag once rather than repeating it:

```bash
# In your agent's shell profile or orchestration script
alias cn='cn --toon'
```

Or in agent orchestration tools (ralph-tui, etc.), configure the `--toon` flag in the tool definition so agents always get compact output.

### TOON format reference

**List output** (pipe-delimited with header):
```
[id|type|title|pri|status|claimed|blocked_by|parent|archived]
t-abc1|task|Design auth module|p0|open||||false
```

**Action acknowledgment** (colon-delimited):
```
ok:claimed:t-abc1
ok:done:t-abc1
ok:archived:t-abc1
```

**Creation result**:
```
created:task:t-abc1:Design auth module
```

**Detail view** (key:value lines):
```
id:t-abc1
title:Design auth module
type:task
priority:p0
status:open
```
