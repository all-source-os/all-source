# Agent-run evidence and replay design

- Date: 2026-09-14
- Status: scoped design for PRD; implementation not started
- Input: [feedback report](../research/2026-09-14-agent-workflow-feedback-report.md)

## Decision

Ship an opinionated agent-run evidence layer over existing Core events. SDK/MCP logging and read-only playback/comparison come first. Core remains durable source of truth; Query Service remains tenant-scoped read plane. Prime remains optional semantic recall. Do not create a new agent runtime or call projection rebuild “agent replay.”

## Approaches considered

| Approach | Benefit | Cost / reason rejected |
| --- | --- | --- |
| Publish event naming guide only | Fast, no new service behavior | Every builder must implement ordering, redaction, retry, and diff; hard to prove value. |
| Versioned run contract + SDK/MCP helpers + read model | Reuses current architecture and supports multiple agents | Requires schemas, guard tests, query contract, and release smoke tests. **Chosen.** |
| Own entire agent executor | Can enforce every tool and re-run sessions | Large scope, duplicates host runtimes, cannot safely control unwrapped tools. Deferred. |

## Data flow

An instrumented agent starts with stable `run_id`, records code-probe evidence, then appends a `change.proposed` event **before** editing. Human approval or rejection is separately recorded. Each attempt appends `attempt.started` before mutation, then test outcome and either accepted, reverted, failed, or abandoned outcome. External tool calls use write-before-execute and an idempotency key. SDK guard reads attempt history and returns allow/stop/unknown. It never executes the tool itself unless caller passes a wrapped action. The generic event API and MCP ingest tool remain available; typed helpers keep event shapes consistent.

Every run uses one tenant-scoped entity stream. Event IDs and server-assigned versions/timestamps are authoritative. Request/response records carry `run_id`, `change_id`, `attempt_id`, `causation_id`, `agent_id`, prompt/model **identifiers**, and schema version. Raw prompts, code, arguments, personal information, and secrets stay client-side by default; only hashes, small redacted summaries, evidence references, and outcomes cross the boundary. Unknown/capture-gap is an explicit state.

Query Service builds a rebuildable run-summary projection and exposes paginated run timeline plus read-only comparison. Comparison aligns change number, input/evidence hashes, prompt/model identifiers, test fingerprints, and outcomes; it reports first divergence and missing evidence. It does not rerun tools. A separate future sandbox runner could consume fixtures and write a linked candidate run, but only after explicit side-effect isolation is proven.

## Error and safety handling

- If before-edit append fails, guarded execution does not proceed; unguarded callers get a visible capture-gap warning.
- If result append fails after action, state is `unknown` until external provider or human reconciliation; never blindly retry side-effectful action.
- After configured failed-attempt cap, wrapped calls return `stop` and no third action executes. Cap belongs to caller policy, not generic Core ingestion.
- A saved revert appends a compensating event linked to original attempt; history remains immutable.
- Cross-tenant reads/writes fail at gateway boundary. MCP needs explicit least-privilege write scope; local read-only MCP cannot record attempts.
- Server rejects oversize or disallowed sensitive fields in structured agent-event helpers. Analytics receive aggregate status counts only.
- Run playback and diff remain read-only even when user has write credentials.

## Validation and handoff

Test reducer/order/dedup and stop rules at unit level; schema and auth at contract level; SDK/MCP → gateway → Core → Query Service at integration level; one fake-agent run through crash, two failed attempts, stop, and run comparison at E2E level. Release only after hosted MCP artifact/tenant smoke test or mark REST/SDK as supported hosted path. See [PRD](../proposals/prd-agent-run-evidence-and-replay.md) for stories and quality gates.

Source paths: `apps/core/src/store.rs`, `apps/core/src/application/dto/event_dto.rs`, `apps/mcp-server-elixir/lib/mcp_server_elixir/protocol/mcp_tools.ex`, `apps/query-service/README.md`, `sdks/typescript/src/client.ts`, `sdks/rust/src/client.rs`, `apps/prime-mcp/README.md`, `apps/web/src/app/(marketing)/docs/mcp/page.tsx`.
