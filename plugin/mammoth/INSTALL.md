# Installing mammoth across agents

mammoth's core capability is one MCP server — `allsource-prime`. Any MCP-capable
agent gets all 23 `prime_*` memory tools natively by adding one server stanza. No
per-agent shim for the core capability.

## Prerequisite (all agents)

```bash
cargo install allsource-prime   # needs >= 0.21.6 (fixed compressed index + in-process fastembed)
```

The binary must be on your `PATH`. Memory is **local-only by default** — a
`.prime/` data dir on your machine, durable (WAL + Parquet), no account.

## The MCP stanza

Every agent below uses the same server. `--auto-inject` exposes a compressed
knowledge index as the `prime://auto-context` resource, injected into the model's
context each conversation.

```jsonc
{
  "mcpServers": {
    "prime": {
      "command": "allsource-prime",
      "args": ["--data-dir", "~/.prime/memory", "--auto-inject", "--auto-inject-max-tokens", "1000"]
    }
  }
}
```

Use an absolute or per-project data dir as you prefer (e.g.
`${CLAUDE_PROJECT_DIR}/.prime` for a project-scoped Claude Code store). To upgrade
to cross-machine/team memory, add `"--sync-to", "<core-url>", "--api-key", "<key>"`.

## Install matrix

| Agent | Where the stanza goes | Notes |
|-------|----------------------|-------|
| **Claude Code** | Plugin (recommended): `/plugin marketplace add all-source-os/all-source` → `/plugin install mammoth`. Or project `.mcp.json`. | Richest surface — MCP + the `mammoth-memory` skill + `/remember` `/recall` `/memory-status`. Approve the `prime` server on restart. |
| **Cursor** | `.cursor/mcp.json` (project) or `~/.cursor/mcp.json` (global) | Native MCP. `prime_*` tools appear after reload. |
| **Cline** (VS Code) | Cline MCP settings → `cline_mcp_settings.json` | Native MCP. |
| **Windsurf** | `~/.codeium/windsurf/mcp_config.json` | Native MCP. Uses the same `mcpServers` shape. |
| **Codex** | Codex MCP config (`config.toml` `[mcp_servers]` / JSON per your version) | Verify the tool-call surface on first run before relying on it. |
| **Gemini CLI** | `~/.gemini/settings.json` → `mcpServers` | Native MCP. Verify the `prime://auto-context` resource is honored. |
| **Any other MCP agent** | Its `mcpServers` config | One stanza, all 23 `prime_*` tools (plus `inbox_*` and `hound_*`). |
| **Agents without MCP** | Use `chronis` (`cn`) for episodic/task memory at the shell level | Only place a per-agent path is needed; keep it minimal. |

### Verification (any agent)

After adding the stanza and reloading, confirm tools named `prime_recall`,
`prime_context`, `prime_stats`, `prime_add_node`, `prime_embed` are available, then
ask the agent to record something and recall it in a later session. **Do not list
an agent as supported until its recall path is verified end-to-end** — a broken
row erodes trust.

## How memory persists

Local-only `--data-dir` mode is genuinely durable: the same AllSource Core engine
(WAL with CRC32 + fsync, Snappy Parquet snapshots) that powers the hosted product.
Your memory survives restarts and never leaves your machine unless you turn on
sync.

> Free tier remembers across sessions on *this machine*, no account. Cross-machine
> and team memory need a free AllSource account.
