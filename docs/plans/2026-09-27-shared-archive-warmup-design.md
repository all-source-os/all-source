# Bounded cache warming for strict HTTP operations

## Observed failure

Release 45 production evidence records a healthy tenant's 90,064-event cold
load taking 10.19 seconds. A new synthetic reproduction with that event count
and 16,100 files fails under the new strict default after 4.01 seconds with
`Strict archive read budget exceeded: elapsed time`; no history becomes resident
and no event is appended. Repeating the same request would restart the same
bounded read, so this is an availability regression rather than a transient wait.

## Decision

Keep short response deadlines and strict integrity. Before a production HTTP
conditional append or strict retained read, let its already-admitted blocking
worker warm the tenant's verified archive with a separate 30-second cooperative
deadline. The existing file, entry, compressed/uncompressed and row caps still
apply. This is service-owned cache work, not permission to finish a timed-out
write or disclose a result after its caller leaves.

Raising every response timeout would keep callers waiting and conflict with the
customer store's transport bound. A new persistent entity index could eliminate
whole-tenant cold reads but requires a separate crash/compaction/retention design.
Bounded cache warming uses the existing loader, generation checks and residency
guards without adding a durable store, queue, retry or index format.

## Deployment worker bound

`ALLSOURCE_ARCHIVE_WORKERS` selects one or two strict HTTP workers per process.
Absent configuration preserves the original two-worker default. Invalid values,
including zero and non-Unicode values, restrict admission to one worker; they
cannot disable the bound or silently exceed the supported maximum. Configuration
is read once when the shared pool is created, not from request data.

The Fly configuration selects one worker. A fresh native measurement of the
existing dense archive shape consumed 1.33 GiB peak RSS with the server allocator.
Two such loaders plus the configured 2 GiB cache have no demonstrated headroom
on the 4 GiB host. Serializing strict operations bounds that staging concurrency;
the worker still retains its permit until it exits after timeout or disconnect.
Sixteen waiting slots, 100 ms admission and 5 s responses remain unchanged.

This setting does not bound generic legacy loading, total process RSS or the
soft cache budget. It does not raise the 250,000-row limit. A constrained Linux
capacity result is still required before changing row admission. The actual
Core HTTP/restart fixture selects one worker to exercise the deployment setting,
including its existing concurrent command burst and recovery assertions.

## Lifecycle

1. Validate the tenant and observe request cancellation before starting warmup.
2. Hold the same worker permit through warmup and the requested operation.
   There remain at most two active workers and sixteen short-lived waiters.
3. The existing tenant load lock coalesces concurrent hydration. Warmup ignores
   request cancellation while reading; its own time/input budget remains active.
   It can complete verification for later requests even after the first caller's
   five-second response deadline. Failed/partial loads never become authoritative.
4. Recheck request cancellation after warmup, before invoking the requested
   append/read. Existing checks before WAL and before returning a snapshot remain.
   No automatic append or read retry is introduced.
5. A later request must pass current authorization independently. It may use the
   completed verified cache, while eviction still forces fresh verification.

`EventStoreConfig.http_archive_warmup_timeout` is an embedding setting, not a
request field. It defaults off for embedded stores and is enabled at 30 seconds
by server environment construction when Parquet storage is configured. The
four-second direct strict-operation budget remains unchanged. No runtime
environment override or caller-provided budget is introduced in this increment.

## Verification and remaining limits

Use controlled lock contention to prove a response can time out while its worker
retains capacity; releasing the lock completes cache verification but never
appends the cancelled command. A subsequent separately issued command must see
the correct predecessor version. Verify corruption and input-budget failures
still refuse, warm requests and ordinary writes retain behavior, and actual HTTP
readiness/restart/concurrency cases pass against the rebuilt binary.

Explicitly run the synthetic 16,100-file probe with the production HTTP policy.
Its initial direct-store refusal remains a useful control; warming must make
that same retained history usable without dropping entries or relaxing counts.
Kernel I/O remains cooperatively bounded rather than forcibly cancellable.

This does not admit arbitrarily large archives. The production metadata scan
found one tenant above file/byte caps; identifying its operational use and the
remaining row/uncompressed budgets is still required before rollout. It does
not resolve retained-history high-water marks or authorize customer activation.

Source boundaries: `apps/core/src/store.rs`,
`apps/core/src/infrastructure/web/archive_work.rs`,
`apps/core/src/store_archive_work_tests.rs`,
`apps/core/tests/archive_compatibility.rs`.
