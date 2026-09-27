# Strict retained-entity read contract

Date: 27 September 2026. Open task: `t-e1d5cf`, within the customer-agent delivery
goal. This increment adds explicit integrity-sensitive reads; it does not enable
customer tools, source disclosure, human approval or product actions.

## Failure and behavior

Five adapter regressions failed before the client change: the request omitted
the strict selector, and complete-looking empty responses with a missing,
wrong-tenant, wrong-entity or unsupported-protocol attestation were accepted.
All fifteen adapter cases pass after requiring the bound protocol marker.

`integrity=retained-entity-v1` opts into strict archive hydration and a pinned
retained-entity snapshot. Unknown protocols, missing targets/limits, filters,
nonzero offsets and descending order are refused. Generic queries remain
tolerant and omit `archive_integrity`. Strict reads use the bounded archive
worker pool, enforce input/work limits and cancellation, cap index entries before
copying them, and bound serialized event bytes before cloning selected payloads.

The response binds its protocol to the authoritative tenant and exact entity.
It does not use the unlocked entity version counter. The customer run adapter
also checks complete-page counts and its 1,000-event/2 MiB limits. An old Core
response without the marker fails closed, including when its event list is empty.

## Verification

- All-feature Core library: 2,009 passed, five pre-existing ignored.
- Six store integration tests passed: damaged archive refusal before/after a
  tolerant read, buffered/flushed/evicted/reopened history, tenant/entity/scope
  enforcement, entry limit, encoded payload/metadata limits and invalid inputs.
- Two real-handler contract tests passed, including eighteen malformed/filtered
  query forms, valid empty history, bounded pages and legacy field omission.
- A controlled materialization-lock fixture proved eviction waits for the
  snapshot lease and cancellation prevents returning a successful snapshot.
  Its lock holder was moved to an owned thread with a three-second deadline;
  the final fixture passed after that test-only refinement.
- All-target/all-feature Clippy with warnings denied and Rust formatting passed.
- Query Service: six doctests and 1,112 tests passed, two skipped and 131
  integration-tagged cases excluded. Formatting, warnings-as-errors compilation
  and strict Credo passed.
- The rebuilt enterprise + analytics Core binary passed **all sixteen actual
  authenticated HTTP/restart tests**, seed 342434, in 15.7 seconds. New cases
  prove bound empty/retained reads across restart and refusal of a corrupt
  archive before and after tolerant reads, across two restarts. Existing tests
  cover corrupt/oversized conditional refusal, ordinary writes, concurrent
  commands, acknowledgement recovery and pending review evidence.

The full Query Service run emitted existing missing OpenAPI operation-spec
warnings. Rust reported dependency future-compatibility warnings; a local
test-only link reported a large unwind section. All commands above exited zero.
No runtime startup failure occurred in this final HTTP run.

## Deployment and limits

Production release 46 remains the earlier `0b51f7d4` decoder/enumeration repair.
The separate image build from `71230434` contains budget/cache repairs but does
not contain this protocol. Neither candidate is proof of this protocol deployed.

The four-second cold-work default may refuse legitimate large/slow archives.
Existing Resend webhook ingestion conditionally appends into routed customer
tenants, so compatibility must be assessed beyond the small operator tenants.
Limits bound input and cooperative work, not exact RSS or kernel I/O duration.
The strict snapshot does not prove retention never deleted events or provide a
durable sequence high-water mark. Entity-keyed counters, externally changed
archives, projection reapplication, customer approval/display and host journeys
remain separate work. Customer activation remains disabled.

The source and local binary manifests identify proof inputs. Temporary logs:
`/private/tmp/strict-read-{client-red,client-green,core,snapshot,clippy,elixir,binary,http,credo}.log`
and `/private/tmp/strict-retained-entity.log`. The committed tests are the durable
reproduction; fixtures use synthetic credentials and owned temporary archives.

## Post-commit rollout checks

Signed source `205422471931a13a7120e6097cce894e62f8b464` was verified equal to
`origin/main`. All source manifest entries passed in clean export
`/private/tmp/allsource-core-20542247-ACb9tL` before starting its build-only
runtime-alpine image. This is not a production deployment.

The additional required Dialyzer gate passed with the existing ignore file
unchanged: seven existing filtered findings and one unused-filter diagnostic.
Log: `/private/tmp/strict-read-dialyzer.log`.

A bounded, read-only production filesystem metadata scan completed successfully.
It used a shell one-liner rather than installing a diagnostic runtime on the
production machine, and returned only aggregate counts/bytes:

| Measurement | Observed |
| --- | ---: |
| Tenant archive directories containing Parquet | 64 |
| Parquet files | 246,215 |
| Total compressed bytes | 1,293,801,569 |
| Tenants above 50,000-file cap | 1 |
| Tenants above 256 MiB total compressed cap | 1 |
| Tenants with a file above 32 MiB | 0 |
| Largest tenant file count | 219,353 |
| Largest tenant compressed bytes | 1,092,911,296 |

This scan did not decode payloads, identify customer tenants in its output,
measure cold decode latency or prove every archive fits the row/uncompressed
budgets. Gateway backend/WebSocket readiness remained healthy at
2026-09-27T14:28:13Z. Production still runs release 46. The observation confirms
that the new defaults are not universally compatible; rollout remains pending.

Compatibility review also found a separate destructive compaction path: a
selected unreadable input was skipped during decode but later removed with the
other originals. Synthetic reproduction and repair are tracked as `t-079e5c`.
The image building from this document's original source does not contain that
later repair. No production compaction was invoked.
