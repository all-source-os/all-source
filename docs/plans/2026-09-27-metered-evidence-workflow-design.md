# Metered customer evidence workflow

Continue `t-7ce02d` and `t-7b4430` through the existing source-sharing and pending
review application services. Customer transports remain disabled until the
remaining delivery gates pass. Core is the only durable store.

## Stable identity and reservation

Use immutable `UTC-seconds:UUID` request IDs for the still-internal source,
preparation and review-read interfaces. The product and MCP binding must reuse
the exact ID on retries. IDs expire after one hour and cannot be revived by
pruning server metadata. A bare UUID cannot provide that bounded guarantee;
regenerating a timestamp on every retry risks a second charge. An unbounded
permanent UUID journal would avoid expiry but add unbounded system metadata.

Derive a tenant/owner/grant/host/purpose-bound Core operation ID from that request
identity. Bind its fingerprint to the exact validated intent and planned query
count. Before admission, conditionally persist the complete meter request,
including the observed period, in an administrative Core config record. Preserve
that original period through uncertain replies and resets. Changed intent under
the same key conflicts before source work. The journal holds at most 192 live
operations per tenant and remains below the existing 60 KB config value bound.
This is an operational bound, not a new commercial entitlement or billing meter.

## Workflow

- Validate source sharing and current consent/ownership, reserve one query unit,
  then read and verify the exact run pin before persisting its source reference.
- Validate a comparison and both current source references, reserve two query
  units, then read the runs and persist the pending comparison.
- A live review read reserves two units before re-reading its pinned evidence;
  expired or revoked metadata-only status responses do not consume query units.
- Successful quota admission counts the planned query attempt, including a
  later source/storage failure. No silent refund or money movement is added.

Current grant, tenant, membership, subscription/trial, consent and source checks
remain before and after work. For these metered services, eligibility checks
validate canonical quota metadata but leave quota availability to atomic Core
admission. Existing connection/context checks retain their current budget rule.
This prevents the final admitted unit from making its own post-read check fail.

## Verification and remaining gates

Prove exact charges through source/share/prepare/read, retries after uncertain
journal and meter replies, simultaneous preparation, Core hard restart, changed
intent, old periods, journal capacity/expiry, malformed state and source/access
revocation. Observe source read counts to prove refused admission cannot perform
heavy work. Keep compile, strict lint, full tests and actual Core proof.

This increment does not claim bounded work across service replicas or completed
cancellation, billing reset integration, source transport, MCP operations, human
approval/display, skills or actual customer-host verification. Those gates remain
required before activation; no production customer flags change here.
