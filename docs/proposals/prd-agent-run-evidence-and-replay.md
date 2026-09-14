[PRD]
# PRD: Agent-run evidence and replay comparison

- Date: 2026-09-14
- Status: proposed; MCP/SDK-first scope confirmed, quality/privacy defaults pending confirmation
- Chronis epic: `t-e3d99f`
- Research: [X feedback report](../research/2026-09-14-agent-workflow-feedback-report.md)
- Design: [agent-run evidence design](../plans/2026-09-14-agent-run-evidence-design.md)

## Introduction

Agent builders need a durable answer to “what did the agent try, what changed, what failed, what was undone, and where did a new prompt/model diverge?” AllSource can store and query generic events today, but builders must invent the run schema, recovery logic, retry boundary, and comparison. This feature packages that work as versioned contracts and tenant-safe SDK/MCP capabilities. It does **not** become an agent executor. Linked X discussion is discovery input, not a customer request or proof of demand.

## Goals

- Instrument a bounded change before an edit and retain approved, failed, abandoned, and reverted attempts through restart.
- Return an ordered, paginated tenant-scoped run timeline with explicit capture gaps and evidence provenance.
- Return a read-only comparison of two recorded runs, including first divergence in changes, tests, and outcomes, without calling external tools.
- Provide a configurable retry guard for **wrapped** actions that stops after configured failed attempts and distinguishes unknown external outcomes.
- Make one hosted production path usable through SDK/REST immediately; gate hosted MCP claims on released artifact and authenticated tenant smoke tests.
- Show one complete fake-data example for Claude Code plus a framework-neutral SDK example. Measure first-run completion and repeat use without ingesting sensitive payloads into analytics.

## Non-goals

- Running arbitrary agents, editing files, or enforcing policy over tools not routed through a wrapper.
- Capturing unsaved or pre-save undone work without an acknowledged before-edit event.
- Live re-execution of tools, payment, email, deploy, or any external side effect during playback.
- Full prompt/source/tool-argument storage by default, autonomous approval, generic tracing/APM replacement, or a new persistence database.
- Claiming exactly-once external effects based on AllSource's event idempotency registry alone.

## Users and scenarios

1. **Solo builder:** Inspect one numbered change, its plan approval, actual test, and revert after a failed device check.
2. **Agent integrator:** Wrap an existing tool; after two failed attempts, stop it and surface a reason to a human.
3. **Engineering lead:** Compare previous and candidate prompt/model runs without executing tools; identify first decision/test divergence.
4. **Operator:** Restore the exact run timeline after client restart and see whether a tool outcome remains unknown.

## Functional requirements

1. Versioned agent-run event schemas with stable run/change/attempt IDs, causation links, agent and prompt/model identifiers, server-acknowledged ordering, and explicit `unknown`/`capture_gap` states. Namespaces must not collide with existing `agent.tool_call.*` examples.
2. Before-action append acknowledgment is required for guarded edits/tool calls. Result append failure after action creates an unknown state; caller must reconcile at source.
3. Reversion appends a compensating event referencing original attempt; original history remains immutable.
4. Retry policy counts failed attempts for a specific change/tool key, defaults to two for provided example, and is configurable. Only SDK-wrapped calls receive hard-stop behavior.
5. Tenant-scoped run timeline is paginated and ordered by authoritative per-entity version/sequence, not client clock. Missing versions or conflicting event order must fail closed or mark order uncertain; no silent timestamp sort.
6. Read-only run comparison aligns by change number/ID and evidence fingerprint; returns first divergence, changed outcomes, absent evidence, and versions used. Comparison never calls an agent or external tool.
7. MCP workflow tools expose clear input schemas and bounded outputs in an opt-in narrow profile, rather than adding every descriptor to the default 55-tool context. Write tools require explicit write permission. Local eight-tool read-only MCP stays read-only.
8. Client redaction allowlist, server size limits, tenant isolation, no credentials/raw source in examples or telemetry. Retention/export behavior is documented against existing plan limits.
9. Document difference among event playback, projection rebuild, and future sandboxed prompt/model replay. Do not call these one feature.
10. Production activation is counted only after authenticated, non-demo run events and successful timeline read. No raw event payloads in PostHog/GA4.

## Quality Gates

### Epic-Level (run once on epic completion)

- Execute repository CI-equivalent for touched surfaces: Rust Core/SDK tests and clippy, Elixir MCP/Query Service tests and formatter, TypeScript/Python/Go SDK tests and type/lint checks, plus web build/tests only if UI/docs code changes. Use current repository commands from each package, not assumed commands; record exact invocations and results.
- Run security/tenant isolation integration suite, one restart recovery test, one no-side-effect replay test, and one hosted connector release smoke test (or clearly ship SDK/REST only until MCP passes).
- Testing trophy complete across layers below; generated fixtures contain no secrets or personal data.
- Review public docs for unsupported query parameters, release claims, retention promises, and terminology.

### Story-Level (checked per story)

- **Schema/backend:** Red-green tests for state transition, ordering, validation, and rejection behavior.
- **SDK/MCP:** Contract tests against real handler shapes, including auth failures and bounded responses.
- **Read API:** Endpoint test proves cross-tenant denial, pagination, deterministic order, and no mutation.
- **Docs/demo:** Reproduce example on clean local setup and, for hosted claims, tenant-scoped production smoke evidence.

## Testing Trophy

### Unit

- Attempt reducer handles proposed/approved/started/tested/accepted/reverted/abandoned/unknown transitions and duplicate input.
- Guard returns allow, stop, or unknown across zero, one, two, and ambiguous attempts; never executes wrapped action on stop.
- Comparator finds first divergence, missing test, reordered/missing event, and non-comparable run.
- Redaction rejects forbidden keys, oversized summaries, and secret-like fixtures.

### Contract/handler

- Core ingest acknowledges version for one run stream; query returns same version/order after restart.
- Query Service timeline and comparison response schemas match TypeScript, Rust, and Python client types.
- MCP tool list advertises permission needs; write call without scope fails; read tool output is bounded.

### Integration

- SDK/MCP → gateway → Core → Query Service round trip with two tenants proves isolation.
- Crash between started and result produces unknown; external reconciliation appends resolved outcome without erasing history.
- Same idempotency key does not license blind external retry after ambiguity or dedup TTL expiry.
- Projection summary rebuild matches event-stream fold.

### Frontend/documentation

- Docs example records fake run and renders readable event timeline and divergence report; links distinguish agent replay from projection rebuild.
- If a UI is added, keyboard navigation, AA contrast, mobile layout, and clear empty/unknown states receive component and browser tests.

### End-to-end

- Fake agent: probe → proposed change → approval → attempt → failed test → revert → second failure → stop → restart → exact timeline → read-only comparison. Stub external action counter proves no third call and no call during comparison.

**Block merge:** authorization, event ordering, redaction, guard, unknown-state recovery, and no-side-effect comparison. **Block release:** hosted connector smoke, SDK/MCP contract parity, restart proof, and documentation accuracy. Visual refinements may follow only if no safety or navigation defect remains.

## User stories

### US-001: Define versioned agent-run event contract [Schema]
**Description:** As an integrator, I want one documented event vocabulary so runs from different agents remain queryable and comparable.

**Acceptance Criteria:**
- [ ] Test first: accepted and rejected fixtures cover required IDs, schema version, lifecycle states, and redaction rules.
- [ ] Define schemas/examples for run, change, attempt, approval, test, tool-call, revert, and capture-gap events; keep Core generic ingestion backward-compatible.
- [ ] Document `run_id`, `change_id`, `attempt_id`, causation, prompt/model identifier, evidence hash, idempotency key, and server-version semantics.
- [ ] Contract test verifies full example sequence can be ingested and queried in exact order after Core restart.

Mark each item [x] as you complete it. Only close when all are checked.

### US-002: Build run fold and read-only timeline API [Backend]
**Description:** As an operator, I want a trustworthy run timeline so I can see accepted and failed work without reconstructing it by hand.

**Acceptance Criteria:**
- [ ] Test first: fold covers every lifecycle state, unknown result, saved revert, pre-save captured attempt, duplicate event, and capture gap.
- [ ] Tenant-scoped API returns ordered, paginated events and summary with server-version cursor, evidence provenance, and `order_uncertain` when ordering proof is unavailable.
- [ ] Cross-tenant ID, forged tenant field, and missing auth cannot disclose run data.
- [ ] Read endpoint performs no writes and passes handler/integration tests.

Mark each item [x] as you complete it. Only close when all are checked.

### US-003: Add typed run recorder and retry guard to TypeScript SDK [Integration]
**Description:** As a TypeScript agent builder, I want to record one bounded change and guard retries without writing manual event code.

**Acceptance Criteria:**
- [ ] Test first: mock client records before-action event before invoking wrapped action; failed append prevents action.
- [ ] Typed helpers record proposed/approved/started/tested/reverted/accepted/abandoned/unknown with stable IDs and hashes, not raw arguments.
- [ ] Default example cap is two failed attempts; configurable policy returns `stop` and invokes no third action.
- [ ] Crash/ambiguous result returns `unknown` and requires explicit reconciliation before re-execution.
- [ ] Existing generic `ingestEvent`, `queryEvents`, and projection replay APIs remain compatible.

Mark each item [x] as you complete it. Only close when all are checked.

### US-004: Add Rust SDK parity [Integration]
**Description:** As a Rust agent builder, I want the same run and guard contract so behavior does not depend on client language.

**Acceptance Criteria:**
- [ ] Test first: Rust contract fixtures match TypeScript event JSON and guard outcomes.
- [ ] Rust helpers expose typed run capture, timeline, and retry decision with same redaction defaults.
- [ ] Two failures stop wrapped action; ambiguous external outcome stays unknown.
- [ ] SDK integration test passes against tenant-scoped API without changing existing generic ingest/replay methods.

Mark each item [x] as you complete it. Only close when all are checked.

### US-005: Add agent workflow MCP tools [Integration]
**Description:** As an MCP-capable agent, I want concise workflow verbs so I can record and inspect runs without composing raw event queries.

**Acceptance Criteria:**
- [ ] Test first: tool schemas reject missing IDs, oversize/sensitive payload, and unauthorized write call.
- [ ] Expose bounded tools to record attempt/approval/outcome and read run timeline/guard decision; names and JSON schemas are discoverable through `tools/list`.
- [ ] Opt-in profile exposes only agent-run tools plus required discovery/health tools; `tools/list` test checks count and write/read gating without expanding the default context unexpectedly.
- [ ] A write tool reports acknowledged event ID/version; a failed append reports no permission to proceed.
- [ ] Tool outputs expose capture coverage and unknown states, not a false “complete audit” claim.
- [ ] Hosted route is documented only after released connector, authenticated write/read, and cross-tenant denial smoke tests; until then use SDK/REST.

Mark each item [x] as you complete it. Only close when all are checked.

### US-006: Compare two recorded runs without execution [Backend]
**Description:** As an engineering lead, I want a deterministic divergence report so I can evaluate a prompt/model revision safely.

**Acceptance Criteria:**
- [ ] Test first: same, changed, missing, reordered, and unknown-evidence fixtures return expected first divergence.
- [ ] Tenant-scoped read endpoint accepts two run IDs and returns aligned changes/tests, first divergence, evidence gaps, prompt/model identifiers, and comparison confidence.
- [ ] Neither comparison nor timeline invokes MCP tools, agent execution, projection rebuild, or external side effects (spy assertions).
- [ ] Cross-tenant or unauthorized run IDs fail without exposing whether another tenant's run exists.

Mark each item [x] as you complete it. Only close when all are checked.

### US-007: Add Python and Go SDK parity [Integration]
**Description:** As an agent framework maintainer, I want language parity so I can instrument an existing workload without bespoke glue.

**Acceptance Criteria:**
- [ ] Test first: shared JSON fixtures match schema and timeline/guard responses in Python and Go.
- [ ] Each SDK exposes typed run recorder, timeline reader, and compare client; Python example covers async use.
- [ ] Guard semantics match TypeScript/Rust; no language silently retries ambiguous side effects.
- [ ] Existing generic SDK APIs remain compatible and package-specific tests pass.

Mark each item [x] as you complete it. Only close when all are checked.

### US-008: Publish runnable integration guide and proof [Integration]
**Description:** As a builder, I want a clean example so I can reach first value without inventing schema or risking a live action.

**Acceptance Criteria:**
- [ ] Test first: automated fake-agent fixture executes every documented step on clean local setup.
- [ ] Guide shows SDK/REST hosted route and MCP route only when its release smoke gate passes; least-privilege keys and safe storage are explicit.
- [ ] Demonstration shows one captured pre-save abandon, one saved revert, stop after two failed attempts, restart recovery, and run comparison; no real email/deploy/payment calls.
- [ ] Verify published `sort=asc` sample against actual API; correct or remove it if unsupported, and explain server ordering, retention, capture gaps, and projection-rebuild distinction.
- [ ] Record privacy-safe funnel counts: first authenticated run, completed timeline read, first comparison, and repeat active builder; exclude demo/QA from customer adoption.

Mark each item [x] as you complete it. Only close when all are checked.

## Technical considerations

- **Architecture:** Core generic event write and WAL remain source of truth. Query Service performs tenant-scoped HTTP/realtime/analytics reads and rebuildable summaries. Prime may index selected learnings; it is not action ledger or required dependency.
- **Ordering:** Do not infer exact order from client timestamps. Verify current API exposes entity version end-to-end; if not, add version/cursor contract before comparison ships.
- **Idempotency:** Core metadata registry has a default 24-hour dedup window. Persist stable IDs in run events and reconcile external side effects at provider; do not promise indefinite exactly-once delivery.
- **MCP:** Existing local read-only Rust MCP cannot write. Existing Elixir connector has generic ingest/timeline tools. [Live MCP documentation](https://www.all-source.xyz/docs/mcp), checked 2026-09-14, says stable 0.22.0 cannot authenticate to hosted gateway; 0.23.0 fix is merged but unpublished, and no public multi-tenant MCP URL exists. Test released artifact; do not bypass gateway by pointing at internal Core.
- **Cost:** Bound event payload and pagination, project run summary rather than rescanning whole history. Never sample away approvals, failures, stops, reversions, or capture gaps.
- **Retention:** First-market Indie plan currently states 14-day retention. UX and docs must say comparison is limited to retained history and avoid “forever” promise across hosted tiers. Validate live billing/catalog before launch.

## Success metrics and validation

Instrument aggregate, privacy-safe counts only. Primary activation: qualified non-demo builder records one run with before-change event, test result, and timeline read after restart. Secondary: one run comparison, correct stop after cap in wrapped action, and repeated weekly usage. Quality: 100% of test-fixture guarded actions have acknowledged start event before action; zero third calls in two-failure fixture; zero cross-tenant reads; zero replay side effects. Measure setup friction and capture gaps on real pilots. No target conversion or revenue claim until baseline and three independent builders are observed.

## Open questions / decisions before implementation

1. Should default capture be metadata plus redacted diff *hash/reference* only, or allow opt-in encrypted payload snapshots? Recommended: metadata/hashes first.
2. What exactly constitutes a failed retry for non-idempotent external tools? Recommended: unknown outcomes block until provider/human reconciliation; only confirmed failures count.
3. Which hosted MCP artifact/version has authenticated gateway forwarding? Required release smoke, not a documentation assumption.
4. Should human approval evidence contain provider-specific signature or only actor ID/time? Recommended: actor ID/time first, never claim cryptographic attestation without it.
5. Which run retention tiers support a useful comparison window? Follow live plan limits; allow export before expiry.
6. Should a future opt-in sandbox runner be built? Defer until read-only comparison has repeated use and customers request active regression execution.

## Delivery sequence

**First release:** US-001 → US-002 → US-003/US-004 → US-005 → US-006 → US-008. Ship SDK/REST first if hosted MCP smoke fails; do not represent an unreleased connector as hosted MCP support. **Parity follow-up:** US-007 for Python/Go after first-run contract is stable. No paid or safety claim based on public X replies alone.
[/PRD]
