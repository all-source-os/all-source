# AllSource — customer agent delivery

Founder decision: 25 September 2026. Required target; contract specified and customer skill drafted. Runtime and release evidence below remains separate. Existing [BET](BET.md) price, qualification and commercial gates are unchanged. Earlier optional/deferred integration language describes rollout only, not a waiver.

## Customer job and authority

- Preparation: Retrieve tenant-authorised event/restart/replay evidence and prepare a read-only investigation view using existing MCP infrastructure.
- Product display: Source-linked event timeline, query scope, restart proof and replay-plan differences.
- Human decision: The tenant operator reviews the evidence in the product and explicitly authorises any consequential replay or infrastructure action through its separate human gate.
- Domain boundary: No arbitrary queries, cross-tenant payloads, secrets, replay execution, infrastructure change or subscription action from this customer review skill. Existing ingestion SDK functionality is not redefined by this review-surface amendment.
- Skill package: [customer skill](../skills/allsource-customer/SKILL.md); distribute the complete directory and local references when release gates pass. No public install/discovery endpoint is claimed here.
- MCP contract: [logical operations and bindings](../skills/allsource-customer/references/workflow.md). Use existing product MCP infrastructure where present; this record does not assert those new bindings exist.
- Approval enforcement: [human-only gate contract](../skills/allsource-customer/references/human-gate.md). Agent credentials cannot approve; product binds human actor/role, exact version/hash, action, scope, expiry and replay-resistant receipt. OAuth connection consent is separate.

## Product surface and data

Implement review within the existing product workspace or a clearly authenticated product-owned MCP App view. Show source evidence, calculations, changes, unknowns, entitlement and next action. No new deployed route is claimed. The no-UI fallback is an accessible normal-product review with an opaque reference, never a data-bearing/bearer URL. The human-only gate has the same enforcement in both views.

Pending drafts use the current product persistence/privacy architecture. Any change to a local-only or founder-private boundary needs a specific processing/consent amendment before customer data is sent. This delivery requirement alone does not authorise exporting that data. Product data stays out of analytics and tool metadata. Existing paid/free rules and host commerce restrictions remain binding.

## Independent readiness

| Gate | State | Evidence or remaining work |
|---|---|---|
| Product MCP/HITL contract | specified | This record and bundled references |
| Customer Claude skill | draft | Local portable package; validation is separate from actual use |
| MCP runtime and shared rules | not-tested | Bind/discover real tools and prove typed validation, text fallback and retry |
| Human gate and display | not-tested | Actual product interaction and agent-credential denial needed |
| Claude Code skill + connector | not-tested | Install complete package, connect and complete product handoff |
| claude.ai skill + connector | not-tested | Separate install/connector and real host test |
| Host MCP App | not-tested | Actual host render, accessibility and plain-text fallback |
| Identity, entitlement, privacy, recovery | not-tested | Product-specific access, retention and policy evidence |
| Production discovery/distribution | not-tested | Verified endpoint, binding/config guide and release package |
| Qualified customer outcome | unknown | Existing BET gate evidence remains authoritative |

## Required acceptance

1. Normal customer request produces only a pending proposal and the correct product review/display.
2. Agent credential, forged approved flag and chat assent cannot execute the gated action.
3. Wrong tenant/user, stale revision, expired approval and replay are denied without duplicate effects.
4. Relevant edits require renewed human review; exact approved version matches the delivered result.
5. Missing connector/render or interrupted handoff preserves work and reports a truthful incomplete state.
6. The intended human reviews/edits/rejects/accepts in the product; domain exclusions stay enforced.
7. Data consent, refund/revocation/recovery and complete native/web journeys keep existing promises.
8. Skill install success, tool calls and synthetic demos do not count as customer outcome evidence.

## Work dependency order

Implementation follow-up: Chronis `t-70f73e`, open and unclaimed. It owns MCP binding, product human-gate/display implementation and actual host/security proof; this specification does not complete that task. Existing MCP publication work remains separate.

Resolve existing product release blockers → validate domain/authority contract → bind MCP preparation and result tools → implement product human gate/display → package/distribute skill → prove actual host and failure paths → release only with existing product authority. Reconcile current tracker items before adding implementation tasks; this specification does not create a second queue or mark existing tasks complete.
