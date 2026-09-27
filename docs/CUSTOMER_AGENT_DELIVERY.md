# AllSource — customer agent delivery

Founder decision: 25 September 2026. Required target; contract specified and customer skill drafted. Runtime and release evidence below remains separate. Existing [BET](BET.md) price, qualification and commercial gates are unchanged. Earlier optional/deferred integration language describes rollout only, not a waiver.

## Customer job and authority

- Preparation: Retrieve tenant-authorised event/restart/replay evidence and prepare a read-only investigation view using existing MCP infrastructure.
- Product display: Source-linked event timeline, query scope, restart proof and replay-plan differences.
- Human decision: The tenant operator reviews the evidence in the product and explicitly authorises any consequential replay or infrastructure action through its separate human gate.
- Domain boundary: No arbitrary queries, cross-tenant payloads, secrets, replay execution, infrastructure change or subscription action from this customer review skill. Existing ingestion SDK functionality is not redefined by this review-surface amendment.
- Skill package: [customer skill](../skills/allsource-customer/SKILL.md); distribute the complete directory and local references when release gates pass. No public install/discovery endpoint is claimed here.
- MCP contract: [logical operations and bindings](../skills/allsource-customer/references/workflow.md). The existing Elixir server now has an opt-in profile with two locally verified eligibility/validation bindings. Full preparation and handoff remain unavailable.
- Approval enforcement: [human-only gate contract](../skills/allsource-customer/references/human-gate.md). Agent credentials cannot approve; product binds human actor/role, exact version/hash, action, scope, expiry and replay-resistant receipt. OAuth connection consent is separate.

## Product surface and data

Implement review within the existing product workspace or a clearly authenticated product-owned MCP App view. Show source evidence, calculations, changes, unknowns, entitlement and next action. No new deployed route is claimed. The no-UI fallback is an accessible normal-product review with an opaque reference, never a data-bearing/bearer URL. The human-only gate has the same enforcement in both views.

Pending drafts use the current product persistence/privacy architecture. Any change to a local-only or founder-private boundary needs a specific processing/consent amendment before customer data is sent. This delivery requirement alone does not authorise exporting that data. Product data stays out of analytics and tool metadata. Existing paid/free rules and host commerce restrictions remain binding.

## Independent readiness

| Gate | State | Evidence or remaining work |
|---|---|---|
| Product MCP/HITL contract | specified | This record and bundled references |
| Customer Claude skill | draft | Local portable package; validation is separate from actual use |
| MCP runtime and shared rules | partially verified locally | Compiled stdio and HTTP profiles → real Query Service → Core, two discovered tools, typed validation, matching text/structured results and revoked reconnect denial; no preparation/status/result binding |
| Human gate and display | not-tested | Actual product interaction and agent-credential denial needed |
| Shared run evidence substrate | internal implementation | [Typed conditional capture, bounded timeline and comparison](plans/2026-09-27-agent-run-evidence-contract.md); actual Core concurrency and recovery proof, no source-disclosure route or customer-facing proposal binding yet |
| Source references and pending comparisons | internal implementation | [Owner/grant-bound pins and durable pending records](plans/2026-09-27-customer-evidence-reviews.md), separate evidence consent, restart/retry/deletion proof; [canonical query admission](plans/2026-09-27-metered-evidence-workflow-design.md) and [supervised workflow limits/cancellation](plans/2026-09-27-customer-review-work-bounds-design.md) tested locally. Billing reset adoption, transport, human approval and UI binding remain open |
| Claude Code skill + connector | not-tested | Install complete package, connect and complete product handoff |
| claude.ai skill + connector | not-tested | Separate install/connector and real host test |
| Host MCP App | not-tested | Actual host render, accessibility and plain-text fallback |
| Identity, entitlement, privacy, recovery | partially verified locally | Current membership/billing, consent/grant issuance, owner listing/revoke UI, issuance limits, WAL recovery, verified-email joins, local OS-owner checks and remote S256/code/token/HTTP lifecycle; actual hosts, legacy ownership migration, retention and deployed consistency remain open |
| Production discovery/distribution | not-tested | Verified endpoint, binding/config guide and release package |
| Qualified customer outcome | unknown | Existing BET gate evidence remains authoritative |

Schema implementation: [v1 types and exact boundaries](plans/2026-09-26-customer-agent-contract.md),
with [fixture and existing replay-domain evidence](evidence/2026-09-26-customer-agent-contract/README.md).
The schema alone creates no human authority. Subsequent
[restricted runtime and access evidence](evidence/2026-09-26-customer-agent-live-access/README.md)
covers local eligibility/validation only. Review and connection flags default off. No
customer connection, deployment, complete human journey or outcome is claimed.

[Workspace ownership evidence](evidence/2026-09-27-customer-workspace-ownership/README.md)
covers Control Plane signup through actual Core, conditional configuration,
atomic initial trial metadata and recovery. Core must be upgraded before this
Control Plane change. It does not complete customer connection issuance or
authorize any MCP scope absent from current billing metadata.

[Team membership evidence](evidence/2026-09-27-team-membership/README.md) covers
conditional member edits, single-use admission receipts and verified-email
session switching through the actual website proxy. A local browser journey
used synthetic identities and real Core storage. This is ordinary team setup,
not a customer connection, a consequential-action approval or native Claude
proof. Drain old team writers before deploying the new Control Plane revision;
Core and Query Service must already support the new format.

[Connection consent evidence](evidence/2026-09-27-customer-connections/README.md)
covers synthetic browser issuance, one-time secret display, private receipts and
revocation through actual Query Service/Core. Concurrent issuance and limits
survive Core restart. The local form records versioned host/field consent, but
does not prove an OS owner, install a host connector, prepare a review, or confer
human action authority. Production issuance stays disabled. Unconsented v1
credentials deliberately require reconnect. Remote PKCE and HTTP evidence follows below.

[Local connection installation evidence](evidence/2026-09-27-local-customer-connection/README.md)
adds actual UID/file/link/ACL enforcement and a bounded Rust utility packaged with
the existing Elixir server. The website supplies complete one-time configuration,
a private install command and a credential-free Claude Code registration command.
Synthetic browser copy/paste, packaged installer, compiled MCP and Core checks
passed locally. This is an OS-account boundary, not application attestation or
isolation from other programs sharing that account. Linux MCP quality gates and
all four Docker builds for `ba81761f` passed. Native host proof and release
distribution remain required. Production issuance is not enabled.

[Remote authorization service evidence](evidence/2026-09-27-remote-customer-authorization/README.md)
adds exact hosted-client/redirect/resource S256 checks, encrypted expiring codes,
pending-grant denial and single-use Core activation with replay revocation. Actual
Core crash/restart and existing compiled local MCP regressions passed.

[Remote HTTP evidence](evidence/2026-09-27-customer-remote-http/README.md) adds
default-off public OAuth discovery, encrypted request cookies, verified human
consent, bounded token exchange and the existing MCP profile over HTTP. The
actual local Next → Query Service → compiled MCP → Core fixture passed consent,
CSRF, exchange, tool calls and replay revocation. Browser consent rendered, but
browser-tool form navigation was blocked with `ERR_BLOCKED_BY_CLIENT`; interactive
consent and real Claude host verification are still open. This is synthetic
local proof, not a production connection, source disclosure or action approval.
The shared-tree web build included an unrelated analytics edit; deployment needs
a clean committed-tree build. See the [connection runbook](runbooks/CUSTOMER_REMOTE_CONNECTIONS.md).

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
