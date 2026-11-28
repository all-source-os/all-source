# Local client verification, 26 November 2026

On Prime 0.30.1, a signed-in Codex session saved the Checkout graph from the homepage seed prompt. A fresh Codex session answered the homepage lookup prompt with Payments, the queue decision and ADR-17. Claude Code connected to its registered Prime server. Its signed-in prompt walkthrough remains unverified.

## Signed-in Codex prompt run

Two native Codex CLI 0.172.0 `codex exec --ignore-user-config --ephemeral --skip-git-repo-check --sandbox read-only --json` processes ran the [homepage prompts](../../../apps/web/src/components/sections/harness-home.tsx) verbatim. Both used the same new directory and the released Prime 0.30.1 macOS arm64 binary from the [v0.30.1 release](https://github.com/all-source-os/all-source/releases/tag/v0.30.1), the version `cargo install allsource-prime` resolves to on crates.io. Per-invocation config supplied `allsource-prime --mode mcp --data-dir <absolute-scratch-path>` as `allsource-example`, enabled only `prime_search`, `prime_add_node`, `prime_add_edge`, `prime_neighbors` and `prime_history`, and set `default_tools_approval_mode="approve"` for this disposable server. Without preapproval, the noninteractive policy denies the writes. The installed client's existing ChatGPT login supplied model access. The commands made no persistent registration.

| Session | Completed tool calls | Stored evidence |
|---|---|---|
| Seed, thread `01a219e2-d46b-7842-bc3d-7b307df83523` ([receipt](codex-seed.jsonl)) | 3 `prime_add_node`, 2 `prime_add_edge`, 3 `prime_neighbors` | Created Checkout, Payments and a decision with source ADR-17. Wrote outgoing `owned_by` and `governed_by` edges from Checkout. The model added `domain: engineering` to each node. |
| Fresh lookup, thread `01a219e3-9e9e-7611-a14c-37b4cbae3bd1` ([receipt](codex-recall.jsonl)) | 2 `prime_search`, 6 `prime_neighbors`, 1 `prime_history` | Answered “Payments owns Checkout”, quoted “Queue checkout requests during payment provider outages” as the linked decision and ADR-17 as its `source`. |

Both Codex processes exited with status 0. The fixture contains no full ADR document; the answer recovers the recorded reference only.

[Engineering graph observations](engineering-graph.json) repeat the graph on 0.30.1 without a model: two writer processes, then three fresh readers that discovered Checkout by type and used only IDs returned by the previous read. During the second writer's shutdown, the embedding back-fill logged `spawn_blocking failed: task … was cancelled` for all three nodes. Graph reads were unaffected; semantic recall over these nodes was not checked.

## Fresh-profile registration and cleanup

| Check | Codex CLI 0.163.0 | Claude Code 2.1.312 |
|---|---|---|
| Register the [setup command](../../plans/2026-11-26-cto-landing-design.md#first-screen--proposed-copy) | `codex mcp add` added `allsource-example` to the disposable profile | `claude mcp add --scope local` added it to the disposable project's local config |
| Start Prime | Codex app-server reported 19 tools for the named server | `claude mcp get allsource-example` reported `Connected`, transport `stdio`, and the expected absolute data directory |
| Write and restart | One native app-server thread made three `prime_add_node` and two `prime_add_edge` calls through `mcpServer/tool/call`. Its process exited with status 0. A new process opened a new thread and read the graph. | A separate stdio client invoked the registered Prime command and made the same five writes. Writer and fresh reader both exited with status 0. |
| Remove example | `codex mcp remove allsource-example` left only `unrelated-proof` in `codex mcp list --json` and the profile TOML | `claude mcp remove --scope local allsource-example` left only `unrelated-proof` in `claude mcp list` and the project's configuration |

Each reader started with a service search and used only IDs discovered from that read. The outgoing `owned_by` edge returned the team Payments. The outgoing `governed_by` edge returned “Queue checkout requests during payment provider outages”. The decision's creation event carried that text and `source: ADR-17`.

| Reader | Checkout ID | Decision creation event time (UTC) |
|---|---|---|
| Codex native app-server | `eac63c54-c3b8-4bc1-9767-48b5be5b2705` | `2026-11-26T20:46:31.759578Z` |
| Independent MCP client using Claude's registered command | `c591f1a7-8145-44ca-b87b-ef067f7c36fb` | `2026-11-26T20:46:31.415231Z` |

The registration tests used separate temporary profiles, project directories and synthetic stores, with one Prime process per store at a time. Both used the earlier installed `allsource-prime 0.22.0` binary and have not been repeated on 0.30.1. Package installation was outside this check. A Claude print-mode prompt returned `Not logged in · Please run /login` with zero output tokens. The existing Claude profile also reported `loggedIn: false`. These fixture checks do not measure unaided setup by an engineering team.
