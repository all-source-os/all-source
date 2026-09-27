# Durable review query admission

Implementation dependency of `t-7ce02d` and `t-7b4430`. Customer source and
preparation routes remain disabled until integration and the existing delivery
gates are verified. This design preserves the catalog and canonical
`metadata.quotas.queries_used` billing/dashboard counter.

## Decision

Use the existing Core tenant repository and system WAL. One tenant event commits
the query counter and a bounded retry receipt together, under the same tenant
lock used by ordinary increments and metadata writes. A private second billing
counter would diverge from the dashboard. Making the existing asynchronous
reporter retry longer would still lose buffered work on process death and cannot
atomically decide which concurrent request gets the last quota unit. Reuse the
existing tenant-updated event with an additive receipt field: older readers still
replay the canonical counter. Customer query admission must be disabled on an
old-server rollback because those readers cannot reconstruct retry receipts.

The administrative Core operation accepts an operation ID, exact request digest,
one to four query units and the observed query-period generation. It accepts no caller
quota, subscription or human approval. Query Service will derive these fields
from the authenticated operation; none grants source access by itself.

Operation IDs include their creation time and a UUID. Their retry lifetime is
one hour, checked from that immutable ID; changing the time makes a different
operation. Identical retries inside the window return the durable receipt.
Changed digest/count/period conflicts, expired IDs, malformed canonical quota
metadata, inactive tenants and insufficient quota fail closed. At most 4,096
unexpired receipts are retained per tenant; capacity refusal does not charge.
This is an operational bound, not a new commercial entitlement.

## Metadata and reset boundary

After the first durable admission, Core owns that tenant's query counter.
Ordinary tenant replacement and metadata merge preserve that counter under the
tenant lock. A separate generation starts at zero and is reconstructed from
the same durable receipts. An admin-only snapshot reports it with the canonical
usage and quota. Existing unrelated metadata and ordinary event
meter behavior remain governed by their current APIs. A generic metadata write
cannot erase retry receipts because those are a rebuildable index of dedicated
system-event payload extensions, not caller-editable tenant metadata. Non-admin
tenant patches cannot change the reserved quotas/subscription/overage objects.
The existing HTTP middleware requires admin authority for all tenant metadata
patches, including projection preferences; this increment preserves that boundary.
The handler also refuses reserved billing fields if invoked through a future
tenant-scoped transport.

An explicit administrative reset compares the observed generation and advances
it by exactly one. It resets only the canonical query counter in the same durable
event. Retrying the same transition cannot clear queries charged afterward;
old-period admissions cannot charge the new period. Billing callers must retain
the original expected generation through retries, rather than reading a new one
and inadvertently requesting a second reset. No automatic reset schedule,
money transfer, refund or subscription change is inferred. Billing callers must
adopt this operation before managed query admission is enabled for customers.

The existing `quotas.reset_date` also controls x402 billing-window calculation.
This query-only operation does not read or modify that shared date, event usage,
x402 usage or extraction usage. The private generation is concurrency metadata,
not a second usage counter or a replacement billing calendar. Existing admin
billing metadata updates remain possible; they cannot zero the managed query
counter implicitly. Opaque product metadata continues through existing service
authority; no new direct customer metadata permission is granted.

All tenant mutations share one per-tenant lock so cache reconstruction or tenant
suspension cannot overwrite an admitted counter. Different tenants remain
independent. Unsupported repository implementations refuse this new protocol;
there is no fallback to best-effort metering.

## Core transport

- `GET /api/v1/tenants/{id}/usage/queries`: canonical usage/quota and query
  generation, including whether this meter is already managed.
- `POST /api/v1/tenants/{id}/usage/queries/admit`: closed request containing
  `operation_id` (`UTC-seconds:UUID`), a lowercase SHA-256 `fingerprint`, `count`
  and `expected_period`. A successful response includes
  `protocol: canonical-query-usage-v1`, the exact durable receipt and `replayed`.
- `POST /api/v1/tenants/{id}/usage/queries/reset`: `expected_period` only.
  The caller must retain this exact transition identity through uncertain replies.

All routes require Core administrative service authority. They are not customer
MCP tools and must never be exposed by the customer profile. Bodies are limited
to 2,048 bytes. The leader permits 16 concurrent metering requests across all
Query Service replicas, with immediate busy refusal instead of a wait queue.
These slots bound metering requests, not the later evidence read: end-to-end
source-work concurrency remains required at the service boundary.

Quota exhaustion returns 402; changed operation/period 409; expired operation
410; receipt capacity or busy admission 429; inactive tenant 403; unavailable
storage or malformed canonical quota state 503. No caller-supplied error payload,
credential or source content is logged by these handlers. Existing generic
query and ingestion routes still use their existing reporting path.

## Integration and proof required

Core proof: exact retry after lost acknowledgement and hard reopen, simultaneous
last-unit admission, normal increments mixed with admissions, stale full saves
and metadata patches, reset/retry ordering, malformed/expired operation IDs,
bounded receipt storage and admin-only actual HTTP routes. A receipt establishes
durable quota admission, not successful source retrieval or human approval.

Query Service must admit before each bounded source read, carry stable operation
identity through retries, and distinguish identity/entitlement rechecks from the
already admitted quota unit. Otherwise consuming the final unit would cause the
post-read authorization check to deny that same valid request. Add bounded
multi-instance work admission, current grant/source checks and cancellation
proof before binding MCP preparation/status/result operations. Continue through
product source consent, human review/display, skills and actual hosts; this Core
dependency does not complete those deliverables.
