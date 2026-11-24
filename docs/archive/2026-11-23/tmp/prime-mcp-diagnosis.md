# Prime MCP Local Onboarding — Diagnosis

Phase 1 of `.prompts/001-fix-prime-mcp-local-onboarding.md`. Maps current
state with file:line citations before any code is written.

## 1. Where does the Prime MCP server binary come from?

- **Crate published on crates.io** as `allsource-prime` — current published
  version is `0.21.5` (`cargo search allsource-prime` returns
  `allsource-prime = "0.21.5"`).
- **Source** lives at `apps/prime-mcp/Cargo.toml:1-10` (package name
  `allsource-prime`, binary name `allsource-prime`, MIT/Apache-2 license).
- **Install path today**: `cargo install allsource-prime` — already documented
  in `apps/prime-mcp/README.md:9-12`.
- **Current developer state on this machine**: `which allsource-prime`
  returns `/Users/decebaldobrica/.cargo/bin/allsource-prime`, `--version`
  reports `0.21.4` (one patch behind crates.io but functional). So the
  binary exists locally; it is just not wired into any Claude Code session.
- **Entry point**: `allsource-prime` on `$PATH`; CLI flags defined in
  `apps/prime-mcp/src/main.rs:50-98`. Defaults: `--mode mcp`, talks to
  Claude over stdio.

## 2. What does Prime MCP need at runtime to function locally?

- **Required arg**: `--data-dir <PATH>` (or `PRIME_DATA_DIR=<PATH>`) —
  declared at `apps/prime-mcp/src/main.rs:57-59`. This is the directory
  Prime writes its WAL + Parquet + fastembed embeddings into.
- **It does NOT talk to AllSource Core over HTTP.** The Prime "backend"
  is opened in-process via `allsource_core::prime::Prime::open(&cli.data_dir)`
  at `apps/prime-mcp/src/main.rs:114`. That call boots a full
  embedded `EmbeddedCore` (WAL + Parquet + DashMap) under the hood,
  scoped to `--data-dir`. No bearer token, no localhost:3280, no Core
  process needed.
- **Embeddings**: in-process via `fastembed` (per `apps/prime-mcp/README.md`
  lines 123-147). First call downloads the AllMiniLML6V2 model
  (~25 MB) into the fastembed cache; thereafter ~1-3 ms per call.
- **Optional remote sync**: `--sync-to <URL> --api-key <KEY>` ships
  `prime.*` events to a remote tenant for the web Memory tab
  (`apps/prime-mcp/src/main.rs:85-97`, `155-170`). This is **optional**
  and orthogonal to local read/write — recall and add_node work fully
  offline without it. If only one of the pair is set, startup aborts
  with a clear error (`apps/prime-mcp/src/main.rs:166-168`).
- **No bearer auth at any layer for local operation.** The bearer-token
  pain in the linked thread report was about AllSource Core's HTTP API
  at `localhost:3280` — but Prime MCP doesn't talk to that surface, so
  that whole class of failure is irrelevant to the local onboarding fix.

## 3. Why don't `mcp__prime__*` tools surface in a fresh Claude Code session today?

- **No project-scoped `.mcp.json`** exists in this repo
  (`ls .mcp.json` → "No such file or directory" in repo root). `.mcp.json`
  is even explicitly gitignored at `.gitignore:98`.
- **Claude Code's project-MCP loader expects `.mcp.json` at the project
  root.** Confirmed via `claude mcp add --help` (scope `project` writes
  this file) and `claude mcp list` / `claude mcp get` notes which spawn
  "stdio servers from .mcp.json" for health checks. So with no
  `.mcp.json` present, the loader has nothing to register — zero
  `mcp__prime__*` tools land in the deferred-tools list.
- **No global/user-scope Prime entry either**: nothing matching `prime`
  is registered in `~/.claude/settings.json` MCP block (verified by
  inspection — settings.json is the user-scope settings file and
  contains no `mcpServers.prime` entry).
- **Net cause**: it is not a silent-drop bug. The server simply has
  never been registered with Claude Code at any scope. The `.mcp.json`
  must be created by the developer, and today nothing in the project
  does that for them.

## 4. What does today's onboarding actually do?

- `cn init` lives at `apps/chronis/src/presentation/dispatch.rs:87-94`,
  which delegates to `init_workspace` /
  `init_workspace_with_remote` at
  `apps/chronis/src/infrastructure/workspace.rs:194-243`.
- The init writer (`init_workspace_inner`, same file lines 207-243):
  - creates `.chronis/`,
  - writes `.chronis/config.toml` with mode/instance_id/sync,
  - writes `.chronis/.gitignore` for wal/storage/sync,
  - prints `Initialized chronis workspace in <path>`,
  - prints `Remote sync configured. Run \`cn sync\` to push/pull events.`
    when `--remote` is set.
- **It writes nothing related to MCP.** No `.mcp.json`, no Claude Code
  hooks, no Prime config. The `cn init` command is unaware of Prime.
- The hosted-onboarding skill at
  `.claude/skills/allsource-onboard/SKILL.md` covers AllSource account
  creation + `[sync]` block — also unaware of Prime MCP / `.mcp.json`.

## 5. Smallest cohesive change that closes the gap?

**Recommendation: Option A — extend the existing onboarding command.**

Add a new subcommand `cn prime setup` (parallel to `cn sync`, since
Prime is a sibling capability) that:

1. Locates the workspace root (or accepts `--path`).
2. Writes / merges a `.mcp.json` at that root with a `prime` server
   entry pointing at `allsource-prime` on `$PATH`, with `--data-dir`
   set to `<project>/.chronis/prime/` so it lives alongside the
   chronis event log and follows the same gitignore pattern.
3. Adds `.chronis/prime/` to `.chronis/.gitignore`.
4. Probes `allsource-prime --version` and fails loudly with the
   exact `cargo install allsource-prime` remediation if missing.
5. Is idempotent — re-running merges into an existing `.mcp.json`
   instead of clobbering it, and updates `prime` in place without
   duplication.

**Why not Option B (separate `cn prime setup` only):** Option A as
described above *is* a new subcommand. Option B as worded in the
prompt meant "a dedicated command outside the onboarding flow" —
that's effectively what I'm doing, but I'm keeping `cn init` minimal
and adding `cn prime setup` as the targeted action rather than
silently wiring Prime on every `cn init`. Developers who don't want
Prime should not be force-fed an MCP server entry.

**Why not Option C (in-process inside `cn`):** `cn` is a task CLI,
not an MCP server. Bolting an MCP transport into it would expand
its scope and break the existing isolation. Prime MCP already exists
as a published binary; the onboarding just needs to point Claude
Code at it.

## 6. What does local Core auth actually require today?

**Not relevant to this fix.** Prime MCP does not talk to AllSource
Core over HTTP for the local path; it embeds Core via
`allsource_core::prime::Prime::open()` (see Q2). The bearer-token
issue raised in the linked thread report was a separate (and real)
problem with whatever was pointing at `localhost:3280`, but it does
not block the documented Prime UX of `prime_add_node` +
`prime_recall`.

Concretely: a fresh Claude session that has `mcp__prime__*` wired
through `.mcp.json` → `allsource-prime --data-dir <project>/.chronis/prime`
can call both tools end-to-end with **no Core auth, no remote sync,
no localhost:3280 dependency**. The remote sync flags (Q2) are
optional and addressable in a later pass when the user wants
team-visible memory.

So we do **not** introduce any Core dev-mode bypass in this prompt —
that would weaken hosted Core's auth posture for a problem that
isn't on Prime MCP's critical path.

## Plan

Execute Option A from Q5 with the auth approach from Q6:

1. Add a `cn prime setup` subcommand under
   `apps/chronis/src/presentation/cli.rs` and dispatch in
   `apps/chronis/src/presentation/dispatch.rs`.
2. Implement the writer at
   `apps/chronis/src/infrastructure/prime_setup.rs` (new file): probe
   the binary, read-or-create `.mcp.json`, merge a `prime` entry,
   append `.chronis/prime/` to the chronis gitignore, print a
   "what to do next" line pointing at "open a fresh `claude` session".
3. Update `apps/prime-mcp/README.md` to mention `cn prime setup` as
   the recommended way to wire Prime into Claude Code from a project
   directory (still document the manual `claude_desktop_config.json`
   path for Desktop users).
4. Do **not** touch AllSource Core auth — irrelevant for the local
   path (Q6).
5. Verify in a sibling project per the prompt's Phase 3, capture the
   transcript, and only then declare done.

End of diagnosis.
