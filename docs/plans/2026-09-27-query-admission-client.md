# Query Service canonical query-admission client

Continuation of the durable Core admission dependency for `t-7ce02d` and
`t-7b4430`. Core implementation: `adfcc8161bc4e1b0ff93df1154063d406aa91d6b`.
The customer source/preparation transports remain disabled.

## Implemented boundary

`QueryUsagePort` offers administrative snapshot and admission only. It does not
offer reset, subscription changes, source reads or approval. The adapter uses
the configured Core leader and service credential, with no follower fallback,
automatic retry, redirects or buffered usage fallback. Upstream content is
streamed into a 2,048-byte bound, with a six-second task deadline and shorter
connection/request deadlines. It returns fixed errors rather than upstream text.

The closed `canonical-query-usage-v1` response must match the exact operation ID,
fingerprint, count, expected generation and immutable expiry. The receipt's
counter must be an unsigned integer at least as large as its admitted count.
Expired responses cannot initiate work, even if the admission was committed.
Snapshot metadata must include a valid canonical counter, quota, generation and
management flag. Missing, changed, floating-point or legacy protocol values
cannot silently become successful admission. Error classification also requires
the expected HTTP status and exact error code.

No application service calls this port yet. A receipt establishes accounting,
not membership, entitlement, source consent, successful retrieval or human
authority. Tests exercise the port directly with synthetic data; they do not
establish a completed customer journey.

## Integration still required

1. Establish and retain stable operation identity before the first possible
   charge. Existing internal source UUIDs and review idempotency keys do not
   encode the immutable creation time required by Core. A caller must not create
   a new timestamp, count, digest or generation merely because a reply was lost.
   Resolve this before exposing preparation through MCP or the product.
2. Invoke canonical admission before bounded source work. Keep current identity,
   grant, membership, source revision, consent and entitlement checks on both
   sides of the read. Consuming the final admitted unit must not cause the
   post-read eligibility check to reject that same valid operation.
3. Bound source work across service replicas and prove cancellation. Core's
   metering semaphore limits accounting calls; it is not a lease on subsequent
   service work. Admission alone does not satisfy this requirement.
4. Adopt the explicit query-period transition in the actual billing reset path.
   Do not infer resets from the shared x402 `reset_date` or change another meter.
5. Finish source consent, pending/status/result MCP operations, product human
   review and display, customer skills and actual host verification before
   activation. Existing feature flags stay off and held bets remain held.
