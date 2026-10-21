---
name: allsource-prime-memory
description: Durable cross-session memory for OpenClaw agents via AllSource Prime. Recall decisions, task history and project context from earlier sessions before answering, and record salient new facts as you work, so the user never re-explains the same thing twice. Use at the start of a session to orient, before answering "what did we decide / why did we / have we discussed" questions, and after a non-obvious decision or discovery. Triggers on "remember this", "recall", "what did we decide about X", "load context".
homepage: https://www.all-source.xyz/prime
metadata:
  openclaw:
    requires:
      anyBins:
        - allsource-prime
        - allsource-prime-macos-aarch64
        - allsource-prime-linux-x86_64
    install:
      - id: prime-macos-aarch64
        kind: download
        label: AllSource Prime 0.26.1 (macOS arm64)
        os: [darwin]
        url: https://github.com/all-source-os/all-source/releases/download/v0.26.1/allsource-prime-macos-aarch64.tar.gz
        sha256: e36224ffaff4b4099b35d21b12eb6ec9e2631d298a5f50b8e7ed72378ddda296
        archive: tar.gz
        bins: [allsource-prime-macos-aarch64]
      - id: prime-linux-x86_64
        kind: download
        label: AllSource Prime 0.26.1 (Linux x86_64)
        os: [linux]
        url: https://github.com/all-source-os/all-source/releases/download/v0.26.1/allsource-prime-linux-x86_64.tar.gz
        sha256: 727a44289c5edd4d807ce368ff7490b7683d343d263a75e51169a704337f5ddf
        archive: tar.gz
        bins: [allsource-prime-linux-x86_64]
---

# AllSource Prime memory

AllSource Prime is an MCP server that gives the agent durable memory: a
knowledge graph plus vector recall, stored locally as a write-ahead log and
Parquet files under `~/.prime/memory`. It survives restarts and needs no
account. Its only network use is the one-time embedding-model download in setup
step 2; memories leave the machine only when both `--sync-to` and `--api-key`
are set.

Source and licence (Apache-2.0): https://github.com/all-source-os/all-source

## Setup

The memory tools come from the Prime MCP server, so it must be registered once.

1. Install the binary: the installer above downloads the release for this OS,
   or run `cargo install allsource-prime` (gives a binary named
   `allsource-prime`).
2. Warm the embedding model once, with network access:
   `allsource-prime --mode warm`
3. Register the server in `openclaw.json` (use the full path of the installed
   binary when it is not on `PATH`), then restart the gateway or node:

   ```json5
   {
     mcp: {
       servers: {
         prime: {
           command: "allsource-prime",
           args: ["--data-dir", "~/.prime/memory", "--auto-inject"]
         }
       }
     }
   }
   ```

   On a headless node host the same entry goes under `nodeHost.mcp.servers`.

The tools appear as `prime_*` (`prime_stats`, `prime_context`, `prime_recall`,
`prime_search`, `prime_add_node`, `prime_embed`, `prime_add_edge`,
`prime_neighbors`, …). If they are absent, skip this skill silently unless the
user asks about memory.

## Provenance rule

Record your own analysis: decisions, rationale, gotchas, outcomes, summaries.
Never write secrets, API keys, credentials, raw proprietary source code or full
file contents into Prime unless the user explicitly says so. File paths,
metadata and conclusions are fine.

## Recall

Before answering a question that depends on earlier sessions, and at the start
of a session on a non-trivial topic:

1. `prime_stats`. An empty store means skip recall; never invent prior context.
2. `prime_context` with the user's question as `query` (top 5). For a plain
   "what do I know about X", `prime_recall` with `text` is enough.
3. Use what comes back and say it came from memory ("from an earlier session:
   …"), so the user can see and correct it.

## Record

After a decision, a non-obvious gotcha, a durable project fact, or an outcome:

1. Look for an existing node with `prime_search` or `prime_neighbors`; update it
   rather than duplicating. Otherwise `prime_add_node` with a `type` (`decision`,
   `insight`, `concept`, `project`, `event`, `person`, `metric`) and a `domain`
   property.
2. `prime_embed` with `id` set to the node's `entity_id` and `text` set to the
   written fact (not source code), so semantic recall can find it.
3. `prime_add_edge` to related nodes (`depends_on`, `causes`, `impacts`,
   `requires`).

Do not record transient state, step narration, chit-chat, or anything already
in the code, git history or project docs. A failed write is noted briefly and
never blocks the user's task.

## On request

- "remember this": record the user's words as an `insight` or `decision` node
  and embed it.
- "what did we decide / why / have we discussed": recall first, answer from
  memory, cite it.
- "forget X": find the node and remove it with `prime_forget`.
