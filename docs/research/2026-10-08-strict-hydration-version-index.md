# Reuse verified archive hydration for conditional writes

Conditional writes can fail with `Storage error: Strict archive read budget exceeded: elapsed time` after a successful retained-entity integrity read. The strict read has already decoded and validated the tenant archive, but the independent entity-version index performs another archive scan under its four-second budget.

During complete strict hydration, observe every archived row's version before event-ID deduplication. Publish version-index readiness only after successful archive enumeration, decoding, generation validation and final budget checks. Refuse negative reconstructed versions, which cannot attest the original unsigned persisted version. Tolerant hydration does not certify the index.

Conditional writes retain their existing `expected_version` comparison. Request budgets, corruption checks and cancellation remain unchanged. Writes do not trigger full hydration. A consumer may use the existing bounded strict warmup before a conditional write; a completed warmup then establishes both attestations.

This change does not provide a durable version index or guarantee a first conditional write on a cold process. A restart still requires archive version discovery. Event-cache eviction preserves the independently retained version high-water marks.

## Verification

Seven separate-file regression tests cover reuse with the subsequent archive-scan budget set to zero, duplicate-ID version maxima, other-tenant isolation, tolerant hydration, corrupt/budget/unrepresentable-version failure, cancellation and event-cache eviction. Successful conditional append with that zero scan budget demonstrates reuse of the already verified index.

```text
gtimeout -k 15 300 cargo test -p allsource-core --lib --no-default-features --features server version_index_reuse_tests
```

All seven scoped tests executed and passed. The server-feature test build completed in 55.60 seconds; the tests ran in 0.12 seconds. All 16 existing archive consistency and bounded-work tests also passed in 5.15 seconds, including the HTTP cancellation and health checks. Rustfmt and whitespace checks passed. A read-only review found no CAS regression in the max-version fold, conditional-append lock or eviction path.

The server-feature lint found an existing no-search `async` method with no await. Its replacement retains a future that inserts metadata only when polled. Fifteen search tests passed, including a regression proving that a discarded future leaves metadata untouched. The final Core source passed all 38 scoped tests and `cargo clippy -p allsource-core --lib --no-default-features --features server -- -D warnings`.

The initial no-default-feature library-test build failed because existing test fixtures unconditionally reference server-only authentication, metrics and WebSocket APIs. No tests ran in that configuration. The successful run enabled the server feature, matching the affected deployment path.

No production rollout has occurred. Approval to deploy this repair to the shared Core backend remains pending. Production recovery requires deployment, a verified strict warmup, then successful conditional write and application import receipt readback.
