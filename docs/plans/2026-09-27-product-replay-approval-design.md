# Product-only approval of a tenant rebuild plan

## Contract and source semantics

Implement the existing `start_tenant_projection_rebuild` contract. Its evidence is
the existing bounded 1,000-event replay analysis, not a frozen tenant database.
The reviewed action explicitly reads retained history at dispatch and catches up
live events before atomic publication. Record the exact plan, curated reducer
revision, analysis facts, sample digest, proposing owner, version and validity
interval. Recompute the analysis before a decision; changed facts require renewed
review. Missing total provenance, archive completeness, ordering and restart proof
remain explicit unknowns. Never label the sample a complete tenant snapshot.

The previous tracked-replay design's reference to pinned inputs means pinned
plan/evidence, consistent with the authoritative 26 September contract. It does
not expand the product promise to frozen-history replay. A new full-tenant
snapshot engine would change that contract and duplicate database concerns.

## Human authority and decisions

A product browser request needs its verified session, same-origin protection and
a short-lived server-signed action attestation. Query Service verifies both the
session and an attestation bound to its token, exact operation and canonical body.
Agent credentials, a generic user bearer token, client-supplied approved flags,
and connector consent cannot satisfy this gate. The attestation is not exposed
to JavaScript or returned to the host. Its server secret is distinct from JWT
signing; absent configuration denies actions.

Read current Control Plane-backed membership for every operation. Initial action
approval is restricted to the proposing subject with current `admin` membership;
other users and token role claims cannot substitute. Existing normal SDK/API
behavior remains unchanged. A dedicated default-off flag gates this surface.

Store minimal review metadata in Core conditional configuration. Versioned edits
return the review to pending; approval and rejection race on the same revision.
Each approval receipt binds the exact digest/version, operation, owner/admin,
expiry and one stable replay operation. Only that operation may consume it.
Matching retries recover its existing identity; contradictory decisions conflict.
After an interrupted approval commit, an authenticated exact retry can reserve
the same tracked replay operation. An uncertain dispatch cannot start another
job. Rejection never changes accepted projection state.

## Disclosure, product, and MCP

Extend explicit connection consent before exposing selected replay analysis to a
host. Keep current metadata and selected-run consent behavior compatible. Host
tools can prepare/read pending plans and result receipts, never decide them.
The product displays source scope, sample/unknowns, consequences, exact version,
expiry and current result. It owns edit/reject/approve controls and durable
recovery. Source data remains out of URLs, analytics and immutable review logs.

Reuse existing bounded workflows, Core query admission, source revocation,
curated projections, replay engine and tracked replay journal. Review prepare,
read and freshness checks have fixed query costs and deadlines. Ordinary action
execution retains the existing replay domain's bounds; it is never hidden inside
agent preparation.

## Proof required

Actual Core: competing decisions, unchanged retries, lost acknowledgement,
changed analysis, target edits, expiry, current role revocation, foreign owner and
tenant, cancellation/failure and real replay result. Product transport: missing,
expired, altered and agent-carried action attestations; cookie/origin protection;
no credentials in response. Browser: inspect/share, prepared plan, human decision,
result/reload and rejection, desktop/mobile keyboard access. Compiled MCP verifies
prepare/read-only parity and action denial. Actual host and production release
remain independent gates; synthetic identities do not prove customer outcome.

This design follows `2026-09-26-customer-agent-contract.md` and reuses
`2026-09-27-tracked-replay-design.md`, `CustomerEvidenceReview`,
`CustomerConnections`, `ReplayAnalysis` and `TenantProjections`.
