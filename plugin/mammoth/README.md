# mammoth 🦣

**Durable memory for your coding agent.** Decisions, task history, and code
context from past sessions get remembered and recalled automatically — so you
stop re-explaining your project every single time. Works locally with zero
signup (your memory, your disk, survives reboots), and syncs across machines when
you want it to. Any MCP agent — Claude Code, Cursor, Cline, Windsurf, Codex,
Gemini — speaks it natively.

> *caveman make few token. mammoth never forget token.*

mammoth is the distribution front-door for two AllSource engines:
[`allsource-prime`](https://crates.io/crates/allsource-prime) (semantic recall —
graph + vector + temporal over a durable event store) and
[`chronis`](https://crates.io/crates/chronis) (`cn`, event-sourced task/work
memory). caveman compresses what you *say*; mammoth remembers what you *said*.
They compose.

## The magic moment

Sessions later, you ask *"why did we pick X over Y?"* — and the agent answers
from memory, without you pasting any history. It recalled a decision from a
session it was never in. That's the whole product.

## What you get

- A `mammoth-memory` skill that runs the recall/record loop automatically.
- A `voice-identity` skill + `/voice` command — build a durable, queryable
  **voice file** (how you think, write, and decide) on the same Prime store, and
  have any MCP agent write in your voice by recalling the relevant slice. Replaces
  the viral "100-question interview → static 4k-token markdown blob" pattern with
  something live, portable, and team-shareable. See
  [`docs/proposals/PRIME_VOICE_FILE.md`](../../docs/proposals/PRIME_VOICE_FILE.md)
  and the proof in [`tooling/voice-demo/`](../../tooling/voice-demo/).
- `/remember`, `/recall`, `/memory-status` slash commands (explicit escape hatches).
- The Prime MCP server wired via `.mcp.json` — **23 `prime_*` tools**
  (`prime_recall`, `prime_context`, `prime_add_node`, `prime_embed`,
  `prime_stats`, …) plus the `prime://auto-context` resource: a compressed
  knowledge index auto-injected into every conversation.

## Prerequisite (all agents)

```bash
cargo install allsource-prime   # needs >= 0.21.6 (in-process fastembed, no external service)
```

The binary must be on your `PATH`.

## Install

### Claude Code (richest — plugin)

```
/plugin marketplace add all-source-os/all-source
/plugin install mammoth
```

Restart and approve the project `prime` MCP server when prompted. The
`mammoth-memory` skill then activates automatically.

### One-line installer (any supported agent)

```bash
curl -fsSL https://raw.githubusercontent.com/all-source-os/all-source/main/plugin/mammoth/install.sh | bash
```

Auto-detects your agents, installs the binary if needed, and writes the MCP
config. `--print` to preview, `--help` for flags. Windows: `install.ps1`.

### Install matrix

mammoth's core capability is one MCP server — any MCP agent gets all 23 `prime_*`
tools (plus `inbox_*` and `hound_*`) by adding one stanza. See [INSTALL.md](./INSTALL.md) for per-agent placement.

| Agent | Path |
|-------|------|
| **Claude Code** | plugin (above) or project `.mcp.json` — also gets the skill + slash commands |
| **Cursor** | `.cursor/mcp.json` |
| **Cline** | Cline MCP settings |
| **Windsurf** | `~/.codeium/windsurf/mcp_config.json` |
| **Codex** | Codex MCP config |
| **Gemini CLI** | `~/.gemini/settings.json` |

The shared MCP stanza:

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

## Does it actually work? (benchmarks)

Reproducible harness in [`tooling/mammoth-bench/`](../../tooling/mammoth-bench/)
— run it yourself. Recall is scored against a keyword search+grep baseline over
the same store, with queries deliberately worded *unlike* the stored text.

Full run, 60 memories / 60 queries, k=5 (`bench2.py`):

| metric | result | kind |
|--------|--------|------|
| Recall hit@5 | **0.90** (baseline 0.83, Δ +0.07) | measured |
| Recall hit@3 / MRR | 0.87 / 0.783 | measured |
| Cross-session continuity win-rate | 0.07 | proxy |
| Median tokens saved / recall | 19 (986 total over 54 hits) | estimate |
| Recall latency p50 / p95 | **3.0ms / 3.6ms** | measured |
| Durability (write→restart→read) | **PASS** | measured |

Honest note: on a smaller 18-memory set the edge was +0.17; it narrows to +0.07
as the corpus densifies and the keyword baseline gets more overlap to exploit.
Memory still wins every metric. The improvement levers (hybrid keyword+vector,
graph `depth`, a larger embedder) are unused here — headroom, not yet spent. We
publish the softening rather than hide it.

## How it stores

Local-only by default: `allsource-prime --data-dir <project>/.prime` runs a fully
durable store — WAL (CRC32 + fsync) + Snappy Parquet snapshots, the same
AllSource Core engine. Your memory survives restarts and never leaves your
machine. No account required.

Cross-machine / team memory is the upgrade: add `--sync-to <url> --api-key <key>`
to ship `prime.*` events to a hosted AllSource Core.

> **Free tier** remembers across sessions on *this machine*, no account.
> Cross-machine and team memory need a free AllSource account.

## Provenance

mammoth records its own analysis — decisions, rationale, gotchas, outcomes,
metadata, file paths. It does **not** write raw source code, secrets, or full
file contents into memory unless you explicitly allow it.

## License

MIT (plugin layer — `plugin/mammoth/` + `.claude-plugin/`). The engines:
`allsource-prime` and `allsource-core` are Apache-2.0; `chronis` is MIT. See
[LICENSE](./LICENSE).
