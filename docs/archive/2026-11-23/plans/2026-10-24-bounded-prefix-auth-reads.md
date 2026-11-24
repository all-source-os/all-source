# Bounded event-prefix reads

## Failure

APlaceAhead's OAuth callback reaches an event-type-prefix lookup after consuming its state. On production Core `62b01d9`, that lookup requires whole-tenant hydration and can return 503 when the archive admission estimate exceeds available memory. Single-entity reads alone do not cover this lookup.

## Contract

Event-prefix HTTP queries opting into `integrity=retained-prefix-v1` scan retained archive batches and resident events without hydrating the tenant. Filtering occurs before JSON payload decoding. The read uses the existing strict archive budgets, cancellation and bounded worker pool. It fails closed on corrupt files, exceeded budgets or lock deadlines. It deduplicates event IDs, preserves tenant and request filters, and sorts deterministically before pagination. Returned counts describe the complete selected history, including tombstones. Legacy queries retain their existing contract; unsupported servers reject the new integrity protocol.

Selected results are limited to 10,000 events and 2 MiB of encoded event data. Existing archive row, byte and time ceilings remain mandatory. This is a generic query capability; no customer namespace is embedded in Core. Runtime capacity remains 8 GB.

Complete retained-entity responses also expose the maximum version from the returned snapshot. Partial responses omit it. This lets consumers preserve compare-and-swap writes without pairing an older snapshot with a later version-index observation.

## Verification

Tests exercise the HTTP query path with unrelated large payloads, tenant isolation, descending pagination, archive/live overlap, deletion history, corruption and cancellation. Deploy only after scoped tests, lint, CI and image verification pass. Fresh Google login and loading existing private data remain the final product acceptance check.
