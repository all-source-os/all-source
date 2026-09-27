# Bounded HTTP archive warmup

Date: 27 September 2026. Parent task: `t-e1d5cf`. Design:
`docs/plans/2026-09-27-shared-archive-warmup-design.md` (signed commit
`412a9ae7fc6cf3980664d5ab94ac70c179b19634`). Customer activation remains disabled.

## Reproduction and fix

A synthetic archive containing 90,064 unique events across 16,100 files failed
the direct four-second strict load: no archive became resident and the command
was refused. Earlier production evidence measured a healthy 90,064-event cold
load at 10.19 seconds, so repeating a short cold load cannot reliably recover.

Production HTTP configuration now enables a separately bounded 30-second cache
warmup in the existing admitted blocking worker. HTTP still returns within its
five-second deadline; warmup retains that worker's permit until completion.
The existing tenant lock coalesces hydration. Request cancellation is checked
after warmup, before the requested append/read. Warmup never retries a command.
Read-only writes and invalid query targets are rejected before warming.

Embedded configuration defaults off. File, entry, compressed/uncompressed and
row caps remain unchanged; direct operations retain their four-second budget.
Corrupt or over-budget archives never become authoritative.

## Verification

- All-feature Core library: 2,012 passed, five existing ignored.
- Eleven strict read/handler/compaction regression tests passed.
- Strict all-target/all-feature Clippy and formatting passed.
- Embedded `--no-default-features` check passed.
- Rebuilt enterprise + analytics binary: all sixteen authenticated HTTP/restart
  integration tests passed, seed 342434, in 15.8 seconds.
- Controlled real HTTP test: five-second response timeout while a tenant load
  lock is held; health remains available. Releasing the lock finishes verified
  hydration without appending or publishing the cancelled command. A fresh
  command advances version 1 to 2, confirmed again after reopening the WAL.
- Read-only append, corrupted archive and file-cap refusal preserve zero
  resident events and no subscription publication.
- Explicit capacity probe: 16,100 files / 90,064 events. Direct read refused at
  4.0165 seconds, zero resident events. Cold HTTP returned 503 at 5.0052 seconds;
  verified warmup completed at 5.1854 seconds with all 90,064 events and the
  original version. A separately issued append succeeded at version 2, bringing
  resident count to exactly 90,065. Full probe completed in 18.47 seconds.

The final capacity fixture seeds the real Parquet schema once, then splits its
unique records with ArrowWriter and the production compression setting. This
avoids 16,100 fsync pairs for a read-capacity test; production durability code is
unchanged. Fixture creation took 7.38 seconds, compared with 200.30 seconds in
the original red probe. A faster machine may complete its direct or HTTP read
before those deadlines; the probe validates both successful and timed-out paths.

Temporary logs: `/private/tmp/archive-warmup-{focused,capacity,core,regressions,clippy,binary,http,embedded}.log`.
Initial focused invocation selected zero tests due to a filename/module filter
mismatch; corrected module filter ran all five then-present tests, and the full
suite includes all six final archive worker tests. Rust emitted existing
dependency future-compatibility and test-link unwind-size diagnostics.

## Rollout status and remaining work

Production remains release 46 (`0b51f7d4`). Gateway backend and WebSocket health
passed at 2026-09-27T15:04:18Z. No compaction, retention, customer-data mutation,
customer credentials or feature activation was performed.

The completed older `20542247` build was verified after refreshing registry
authentication:

`registry.fly.io/allsource-core:core-20542247-20260927`

Digest: `sha256:ccba4f3b44fdb6a32328be9eb42faca4fca646d519bc67eac6675256bb398599`.
It excludes both the later compaction repair and this warmup; it was not deployed.
The builder reported a cleanup deadline only after successful image publication.

Latest design commit's CI passed (`36327558668`); older `922a6d6d` CI/security
runs were cancelled as newer commits entered the queue. This is not a claim
that cancelled runs passed or that the uncommitted implementation passed CI.

A read-only count identified the oversized archive as the platform `system`
tenant: 219,389 Parquet files at the later observation, versus the earlier
219,353-file maximum. No customer tenant names or payloads were printed.
This does not prove all operational uses tolerate refusal or all customer
archives fit row/uncompressed budgets. Those compatibility checks, durable
sequence high-water semantics and full customer host journeys remain open.
