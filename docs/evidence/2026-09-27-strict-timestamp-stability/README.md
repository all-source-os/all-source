# Strict evidence timestamp stability

Chronis: `t-417242`, dependency of `t-e1d5cf`.
Base commit: `ebc772d501306d1454c000c1e1b14eb439bb43a7`.

## Failure and repair

CI run `36328511999`, for warmup commit `9a653099`, failed the strict retained
roundtrip on Linux. Its live timestamp `.833006749Z` returned as `.833006Z`
after Parquet reload. The Query Service review digest includes event timestamps,
so equivalent retained evidence could acquire a different digest after restart.

The existing storage schema persists microseconds. Strict `retained-entity-v1`
responses now truncate returned event copies to that precision and sort by
`(timestamp_micros, version)`. Generic queries, cached source events, WAL and
the storage schema remain unchanged. Serialization admission counts the original
event before cloning and therefore remains conservative.

The regression explicitly supplies descending nanosecond timestamps within one
microsecond. This reproduces on macOS, whose clock otherwise often conceals the
Linux failure. Before the fix, versions returned `[3, 2, 1]`; after the fix they
return `[1, 2, 3]`, with identical full snapshots after flush, eviction and reopen.
The generic pre-flush query still exposes the original nanoseconds.

## Verification

| Check | Result |
| --- | --- |
| Forced precision regression before fix | Failed as expected; `regression-before.txt` |
| Strict retained entity suite after fix | 6 passed; `regression-after.txt` |
| All-feature Core tests, including integration suites | 47 suites, 2,349 passed, 14 ignored |
| All-feature/all-target Core Clippy with warnings denied | Passed |
| Rust formatting | Passed |
| Real enterprise + analytics Core binary build | Passed |
| Five Query Service real HTTP/restart suites, seed 342434 | 16 passed; `http-retry.txt` |
| Query Service strict Credo, 295 source files | No issues |
| Changed Elixir test formatting | Passed |

The real HTTP test now compares both the complete Timeline events and its digest
before and after hard process restart, using the rebuilt Core binary through
HTTP rather than importing Core source. Its timestamp assertion also rejects
fractional precision greater than six digits.

The first HTTP run had 15 passing tests and one owned-child readiness timeout
before that test performed its requests. Child output was empty. The same binary,
tests and seed passed on the next run without code changes. Both transcripts are
retained; the cause of that startup delay has not been established.

## Limits

These are local source and actual-process checks. New Linux CI results and
production rollout are separate gates. Production remains release 46. This fix
does not resolve the known 566,486-row archive admission failure, system archive
compatibility, retained-history high-water marks, customer activation, or human
approval UI/host verification. Explicit capacity probes remain ignored by the
default test run; their known-red case is not counted as passing.

`source.sha256` identifies the implementation and regression sources.
