# Strict archive budgets and conditional HTTP work

Date: 27 September 2026. Open task: `t-e1d5cf`, under customer-agent delivery.
This is an incremental safety repair, not completion of the customer flow or
the portfolio goal. Customer evidence capture remains unactivated.

## Implemented boundary

Strict archive reads now have admission limits for entries, files, compressed
file bytes, declared decoded bytes, rows and elapsed cooperative work. The
loader checks cancellation between operations. Conditional single/batch HTTP
appends use two blocking workers, at most 16 queued requests, a 100 ms admission
wait and a five-second response deadline. A timed-out or disconnected caller
does not release the worker's permit until that worker exits.

The store checks cancellation again after checkpoint and entity-version lock
admission, before WAL mutation. Cancellation after mutation begins cannot undo
the append; the HTTP deadline reports an uncertain outcome. Full defaults and
remaining constraints are in the [design](../../plans/2026-09-27-strict-archive-read-design.md).

## Verification

Final local source passed:

| Check | Result |
| --- | --- |
| All-feature Core library | 2,001 passed, five pre-existing ignored |
| Persistence, acknowledgement, concurrency and scope integration suites | 56 passed |
| Rust format and all-target/all-feature Clippy with warnings denied | Passed |
| Rebuilt enterprise + analytics Core binary | Passed |
| Actual Core HTTP/restart suites, seed 342434 | 14 passed, zero failures |
| Query Service format, warnings-as-errors compile and strict Credo | Passed |

Eight focused budget tests cover every limit plus a healthy admitted history.
Refusal leaves no new event or broadcast; tolerant queries still work, a
query-warmed cache still cannot bypass strict admission, and ordinary appends
retain their legacy lazy behavior.

The library includes eight worker/cancellation cases: request runtime remains
available during blocking work; timeout and caller cancellation retain worker
ownership; admission queue capacity and duration are bounded; cancelling a
queued caller releases its queue slot; cancellation before work leaves WAL/cache
empty across reopening; cancellation after hydration/checkpoint contention
prevents append; and an owned, single-thread-runtime loopback HTTP server keeps
health and an unrelated ordinary append available while a conditional request
waits for its tenant load lock. The loopback test uses the existing unversioned
HTTP handler. Versioned production handlers are exercised by the real binary
suite.

The new real-Core case creates a sparse synthetic archive larger than 32 MiB.
Conditional writes refuse it before Parquet decoding, both cold and after a
tolerant query, across two owned Core process starts. Health remains available,
the archive length is unchanged, and no conditional event appears.

The remaining real HTTP cases retain existing concurrency, hard-restart,
version ordering, lost acknowledgement and pending-review checks. Tests use
synthetic credentials/data and private temporary directories; no customer data
was modified for verification.

## Failure retained in the record

The initial two-worker implementation rejected excess callers immediately.
The real HTTP suite returned 13 passes and one failure: simultaneous identical
typed commands produced some `append_uncertain` responses. A bounded 16-slot,
100 ms admission queue fixed that short-burst regression while retaining the
two-worker cap. The original concurrency assertion was not weakened or changed.
The final 14-case run passed after rebuilding the corrected binary.

An earlier fixture compile failed because its constructor name did not exist;
it was corrected to the explicit persisted-store configuration. Strict Clippy
also caught unnecessary cancellation ownership; the internal parameter now
borrows the flag. These were local failures, not production changes.

## Provenance and remaining work

`source-sha256.txt` identifies final local implementation/tests/build inputs;
`local-binary-sha256.txt` identifies the real HTTP proof binary. Verbose local
logs were recorded at `/private/tmp/strict-archive-{all-tests,worker-tests,clippy,binary,http,credo}.log`.
These temporary logs are not durable test artifacts; the versioned tests and
hash manifests are the reproducible record.

The preceding image built from `0b51f7d4a894798cadac13210b34b5520e9a10a9`
has registry digest
`sha256:bdf4922db0d9199b4acc5075fbc2d249c223e4de425b4c1394a12897a25ffd58`.
That image contains archive decoding/enumeration fixes but **does not contain
this budget/worker implementation**. Its production rollout is tracked in the
preceding integrity evidence. Do not substitute its image result for deployment
proof of this source.

Strict evidence query attestation, cache pinning/eviction races, global resident
counter accounting, retained-history/high-water marks and end-to-end customer
approval/display/host proof remain open. The budget bounds input and cooperative
work, not exact RSS or in-flight kernel I/O. Large legitimate archives can be
refused; these defaults are not a claim that existing production archives fit.
No feature flag, credential, human authority or release approval was expanded.

Prime design indexing was attempted through its MCP tool and returned a
read-only-store error. The versioned design remains the durable record; the
other writer was not stopped or bypassed.
