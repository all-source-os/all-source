---
design: docs/plans/2026-11-26-event-history-design.md
design-sha256: 77728334d5a15ba64ff47af3e4c4cec0401331ba138b8d90a4ed675a7678700d
verdict: clean
---

## Round 2

1. **Closed. Timeline coverage.** The strict Core contract now requires a consistent read point, full segment coverage and a half-open interval. The source orders ties by event ID before limiting the page. An unreadable segment yields unavailable. See design line 24.
2. **Closed. Query admission.** Both entry points reserve a query unit before reading evidence. The contract specifies operation identity, duplicate suppression, failure accounting and final-unit contention tests. Browser prefetch opens the shell without spending quota. See design line 26.
3. **Closed. Workspace selection.** Candidate discovery starts with the verified subject. Current membership and entitlement are checked before revealing a workspace name and again when granting access. Tests distinguish two eligible workspaces from stale membership and invalid selection. See design line 22.
4. **Closed. Event projection.** The design names both source event types, their allowed fields and three output variants. Invalid fields produce a generic record and neither release variant supports causal claims. See design line 28.

## Carried to Plan

Check the deployed MCP annotations against two separate calls with identical arguments and different request IDs. Each admitted lookup spends a unit. Retransmitting the same logical operation spends one unit in total. The idempotency annotation must account for both observations. See design lines 26 and 30.

## Round 1 (superseded)

`design-sha256: superseded`
Reviewed digest `2b4c9ae04fa6ebdeebeff48e05f30bbe3fa9a047627d82a0c6b45466736e2263`. Original verdict **findings**.

1. **Blocking. Timeline coverage.** The existing paged reader cannot certify a complete empty window. Core can skip unreadable archive files and return an ordinary successful page. Its strict retained-entity protocol rejects time bounds and descending order. Default ordering breaks timestamp ties by version, and the upper bound is inclusive. Filtering or sorting after the 21-row limit can lose valid rows. Specify a Core contract that guarantees integrity, the half-open interval and the chosen total order before limiting, plus the response evidence required to admit `complete`. Exercise an unreadable segment, an event exactly at the upper bound and a timestamp tie spanning the limit. See `apps/core/src/store.rs:2801`, `apps/core/src/store.rs:302`, `apps/core/src/store.rs:2885` and `apps/core/src/infrastructure/web/retained_query.rs:20`.
2. **Blocking. Query admission and side effects.** The design authorizes a read using entitlement and calls the paged adapter directly. Existing customer evidence reads also reserve query units before source work. Checking eligibility alone lets repeated or concurrent reads bypass that accounting. Specify the charging policy for a host lookup and a browser citation, the admission write and its retry identity, or an explicit product decision exempting either action. Exercise exhausted quota, duplicate delivery and concurrent claims for the last unit through the applicable entry points. See `apps/query-service/lib/query_service_ex/domain/customer_agent/eligibility.ex:20`, `apps/query-service/lib/query_service_ex/application/services/customer_query_admission.ex:6` and `apps/query-service/lib/query_service_ex/application/services/customer_human_evidence.ex:20`.
3. **Blocking. Workspace selection.** The first-use journey promises all eligible workspaces and independent grants. The existing consent service binds to the actor’s single tenant, and the access store reads membership by tenant. Specify how a signed-in subject discovers memberships, how the selected tenant becomes the grant binding, and how the server verifies it at consent time. Cover one subject eligible in two workspaces, a pending invitation and an unrelated tenant. See `apps/query-service/lib/query_service_ex/application/services/customer_remote_authorization.ex:23`, `apps/query-service/lib/query_service_ex/application/services/customer_connections.ex:124` and `apps/query-service/lib/query_service_ex/infrastructure/adapters/customer_agent_access_store.ex:30`.
4. **Should. Event projection.** The proposed output names “typed change fields” but supplies no recognized event-type strings, allowed payload keys or closed response alternatives. A fixture and projector could agree on invented fields and still satisfy the acceptance example. Define the smallest release and rollback variants for the reviewer fixture, including missing or malformed field behavior and which source field can support causal wording. See `docs/plans/2026-11-26-event-history-design.md:7` and `docs/plans/2026-11-26-event-history-design.md:27`.
