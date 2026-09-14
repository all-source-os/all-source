# Agent workflow feedback: X thread → AllSource opportunity

- Date: 2026-09-14
- Source: [Arno's linked reply](https://x.com/meetarno/status/2099516759191511411) and its visible conversation chain
- Status: research and product recommendation, not a customer commitment
- Chronis follow-up: `t-e3d99f`

## Executive finding

The linked post is Arno reacting to a discussion, not asking for an AllSource feature. The useful signal comes from Ed Kunitz describing a Claude Code plugin he built for his own projects and from replies testing its failure modes. His loop is: inspect actual code, name and number one bounded change, confirm the plan with a human, apply it, test on a real device, revert a failed change, and stop after repeated failure. The group then identifies two valuable records: saved reversions and attempts that disappear when undone before save. A replayable history could support regression comparison across prompts and models. These are *workflow needs inferred from discussion*, not proof that participants would buy AllSource.

AllSource already has durable events, tenant-scoped API access, SDK ingestion/query methods, event timeline/time-travel tools, and projection rebuilds. It does **not** yet offer a verified, opinionated agent-run contract and cross-run comparison workflow that makes this loop easy to implement. The right first product is an agent-run evidence layer over Core, not a new agent runtime. See [design](../plans/2026-09-14-agent-run-evidence-design.md) and [PRD](../proposals/prd-agent-run-evidence-and-replay.md).

## Evidence from replies

| Observation | Direct source | Product implication | Confidence |
| --- | --- | --- | --- |
| Ed's plugin probes code before edits, uses a named/numbered bounded change, waits for plan confirmation, and forbids guessing/placeholders. | [Ed describes plugin](https://x.com/KUNUTZ142/status/2098925486214975910) | Capture intent, evidence, approval, and one change boundary per attempt. | High: explicit first-person description. |
| Jovan proposes one patch plus one test per turn and stopping after two failed retries. | [Jovan's reply](https://x.com/jovan_bored/status/2098928332851089644) | Track attempts and policy decisions; make budget configurable. | High for recommendation, not adoption. |
| Ed says a change failing his phone check gets reverted rather than patched further. | [Ed on validation](https://x.com/KUNUTZ142/status/2098941353900077265) | Reversion is a first-class event linked to original change and test evidence. | High: reported practice. |
| Ed says retry cap stopped an agent circling one fix; replayable log emerged as an unexpected benefit. | [Ed on retry cap](https://x.com/KUNUTZ142/status/2099284558831112312) | Show repeated-failure patterns and give caller a hard-stop decision. | High: reported experience; no independent usage data. |
| Ed reports 17 saved-and-undone changes in one repository. Pre-save undo leaves no record. | [Ed on undo timing](https://x.com/KUNUTZ142/status/2099473211456954752) | Emit pre-change attempt events; never claim passive capture of unsaved work. | High for stated semantics; count is self-reported. |
| Decebal proposes replaying old sessions against new prompt/model versions and diffing divergence. | [Regression-testing suggestion](https://x.com/ddonprogramming/status/2099469690145788280) | Store reproducible run references and compare recorded outcomes; side-effectful re-execution needs separate sandbox/approval. | High for proposal, unvalidated as buyer demand. |
| Decebal identifies missing pre-save evidence as place where agent circles and suggests recording failed attempts. | [Missing-attempt suggestion](https://x.com/ddonprogramming/status/2099516381360239080) | Append attempt-start/failure/abandon events before edits and tests, with redacted evidence. | High for design inference. |
| Arno says the exchange was interesting. | [Linked reaction](https://x.com/meetarno/status/2099516759191511411) | Conversation relevance only; no feature endorsement. | High. |

The visible chain starts with [Arno's builder introduction](https://x.com/meetarno/status/2098749581659967754). X's logged-out view limits additional replies; this report traces the linked post's visible parent chain and directly reachable branches, not every possible reply on X. No private data or unpublished plugin code was inspected. Ed says his plugin was [not public at the time](https://x.com/KUNUTZ142/status/2099280215327310053).

## Existing AllSource substrate — checked in repository

| Capability | Evidence | What it does **not** yet prove |
| --- | --- | --- |
| Durable append and optimistic entity version | `apps/core/src/store.rs`, `apps/core/src/application/dto/event_dto.rs` | No agent-specific run/attempt semantics. |
| Event ingestion and query in TypeScript SDK | `sdks/typescript/src/client.ts`, `sdks/typescript/src/types.ts` | No typed run recorder, retry guard, or run diff. Similar raw primitives exist in other SDKs but parity must be tested. |
| Event-store MCP `ingest_event`, `event_timeline`, `analyze_changes` | `apps/mcp-server-elixir/lib/mcp_server_elixir/protocol/mcp_tools.ex` | Generic tools are not an agent workflow contract. Current hosted connector release status must be smoke-tested before any claim of hosted MCP readiness. |
| Local read-only MCP debugging | `docs/guides/ALLSOURCE_MCP.md` | Eight read-only tools cannot record an agent attempt. |
| Prime semantic memory and provenance | `apps/prime-mcp/README.md` | Semantic recall is not a complete action/approval/retry ledger. Keep Prime optional for retrieval. |
| Tenant projection rebuild and SDK replay operations | `docs/plans/2026-08-14-replay-studio-analysis-sdk-design.md`, `sdks/typescript/src/client.ts` | This is **read-model rebuilding**, not agent-session playback or safe prompt/model regression execution. |
| Write-before-execute agent pattern | `apps/web/content/crash-safe-agents-write-before-execute.mdx` | It is an article/pattern, not enforced by SDK/MCP. Its example query includes `sort=asc`; `sdks/typescript/src/types.ts` does not declare `sort`, so published samples need contract verification. |
| Event metadata idempotency registry | `apps/core/src/application/services/exactly_once.rs` | Default dedup window is 24 hours; it is not a durable guarantee against repeating external side effects after that window or after ambiguous tool outcomes. |
| Hosted connector warning | [Live MCP documentation](https://www.all-source.xyz/docs/mcp), checked 2026-09-14; `apps/web/src/app/(marketing)/docs/mcp/page.tsx` | Live page says stable image 0.22.0 lacks gateway auth forwarding, 0.23.0 fix is merged but unpublished, and no public multi-tenant MCP URL exists. Current supported hosted path is authenticated REST/SDK. Recheck before launch. |
| MCP tool context cost | [Live MCP documentation](https://www.all-source.xyz/docs/mcp) | Default connector advertises 55 tools; exposing more by default has agent context cost. New workflow tools should use a narrow, opt-in profile and bounded output. |
| Self-provisioning guidance | [Live MCP documentation](https://www.all-source.xyz/docs/mcp), `apps/web/content/agent-self-provisioning-allsource.mdx` | Public MCP page uses a name-only trial request, while older article shows email plus name. Verify actual endpoint contract before publishing any new agent onboarding recipe. |

## Problem worth solving

An agent builder can write generic AllSource events today, but must invent schema, correlation IDs, state folding, stop rules, redaction, and replay comparison. That creates three failures:

1. **Invisible work:** edits abandoned before save, rejected plans, and failed probes are never recorded unless client emits events before acting.
2. **Unsafe recovery:** retry count and external side-effect status remain ambiguous across crash/restart. Event history can reveal ambiguity, not erase it.
3. **Expensive learning:** saved reversions are visible but not grouped by root cause, prompt/model version, test result, or approved plan. Teams cannot quickly compare where a changed agent diverged.

## Proposed product shape

**First release:** versioned `agent.run.*`, `agent.change.*`, `agent.attempt.*`, `agent.test.*`, `agent.approval.*`, and `agent.tool_call.*` event contract; typed SDK helpers; small MCP workflow tools; tenant-scoped run timeline and read-only run comparison; example Claude Code integration using the existing gateway path once release-safe. Every write includes stable run/change/attempt IDs and an idempotency key. Payloads default to metadata, hashes, statuses, and short redacted summaries, never raw prompts, source files, credentials, or tool arguments.

Minimal captured sequence: `run.started → change.proposed → approval.granted → attempt.started → test.failed → attempt.reverted → attempt.started → test.failed → guard.stopped`. A successful alternative ends `test.passed → change.accepted → run.completed`. These are proposed schema events, not claims that current MCP/SDK helpers already emit them.

**Hard boundary:** AllSource can return `stop` after a configured retry budget only for tool calls routed through its guard/wrapper. It cannot intercept arbitrary editor or MCP calls. Pre-save undo exists in the ledger only if a before-edit event was successfully appended. External actions with uncertain completion remain `unknown` until checked at source. Read-only historical playback never reissues tools. Prompt/model regression execution, if later built, needs sandboxed tool stubs, fixture snapshots, explicit approvals, and provenance linking original to candidate run.

**No new database:** Core stays source of truth; Query Service supplies tenant-scoped HTTP/realtime/analytics reads and rebuildable read models. Prime may index selected learnings but does not replace action history.

## Priority and validation

1. **P0 — prove one real loop:** one Claude Code or SDK agent records plan → approval → attempt → test → revert/accept, survives restart, and reads exact ordered timeline.
2. **P0 — stop safely:** second failed retry yields a stop decision from wrapped execution, with no third external call; ambiguous external outcome yields `unknown`, not automatic retry.
3. **P0 — compare:** two recorded runs produce deterministic, read-only divergence report by change and test, with no tool execution.
4. **P1 — parity and adoption:** Python/Go helpers, install recipe, public demo with fake data, hosted MCP release smoke gate, and run-level analytics.
5. **Later:** managed sandboxed re-execution and automated root-cause clustering only if customers use read-only comparison and request it.

Validate with three independent agent builders, including at least one non-developer using a coding agent. Ask them to instrument an existing workflow without founder-written glue, recover one failed run, and explain one divergence. Record setup friction, time to first valid run, missing-attempt rate, correct-stop rate, and repeated weekly use. Do not treat the X exchange or its view count as paid-demand evidence.

## Risks and countermeasures

- **False audit completeness:** display capture coverage and last acknowledged event; mark gaps explicitly.
- **Secret/IP leakage:** allowlist fields, client redaction before transmission, server size limits, tenant-scoped access, retention controls, no raw payloads in analytics.
- **Retry harm:** SDK guard stops only wrapped calls; never claim exactly-once external effects without provider verification.
- **Replay confusion:** name projection rebuild separately from agent-run playback/comparison throughout docs and tools.
- **Hosted MCP launch claim:** require released artifact, authenticated handshake, write/read round trip, and cross-tenant denial test.
- **Cost:** bounded event size, configurable sampling of benign probes, query pagination, run-summary projection; preserve failed/stop/approval events unsampled.

## Decision

Proceed with opinionated event contract plus thin SDK/MCP workflow layer. Reject docs-only schema (too much per-customer glue) and full agent runtime (too broad, conflicts with existing Claude Code/other runtimes). Keep evidence standard explicit: this is a promising product hypothesis from a public conversation, not a direct purchase signal.
