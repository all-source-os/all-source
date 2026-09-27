# Bounded strict archive worker setting

Part of `t-40896f`; that compatibility task remains open.
Base source: `7a66beb69807e427f711f8ccbdef32641ea52dcd`.

## Change and reason

The service-owned archive worker retains its admission slot until it exits,
including after an HTTP timeout. `ALLSOURCE_ARCHIVE_WORKERS` now allows an
operator to select one or two workers. Absent configuration preserves two;
invalid values restrict admission to one. Zero, oversized counts, whitespace,
non-numeric and non-Unicode settings cannot remove or expand the bound.
The process reads this setting once on first pool creation. Requests cannot set it.

The Fly config selects one worker. A native fresh-process load of the known
dense archive shape used 1,427,767,296 bytes peak RSS with the server allocator.
Two such loads plus existing resident memory have no demonstrated headroom on
the 4 GiB host. One-worker admission limits simultaneous strict staging while
preserving the existing 16 waiting slots, 100 ms admission and 5 s response bounds.

This change does not raise the 250,000-row limit, bound legacy generic loading,
or make the soft cache budget an RSS guarantee. It is not a deployment or
customer-activation change. Production remains release 46.

## Verification

| Check | Result |
| --- | --- |
| Worker configuration, queue and cancellation suite | 6 passed |
| All-feature Core tests and integration suites | 47 suites, 2,350 passed, 14 ignored |
| All-feature/all-target Clippy, warnings denied | Passed |
| Rust and changed Elixir formatting | Passed |
| Enterprise + analytics actual Core binary build | Passed |
| Actual HTTP concurrency/recovery/restart suites, single-worker environment | 16 passed |
| Query Service strict Credo, 295 source files | No issues |

The HTTP fixture explicitly supplies `ALLSOURCE_ARCHIVE_WORKERS=1`, exercising
the deployment setting through its existing concurrent append burst, retries,
evidence review, corruption refusal and hard process restart cases. Agent-run
digest/event equality across restart remains covered.

After full Cargo tests, the enterprise + analytics binary was rebuilt and copied
to an owned immutable temporary path before the final 16-test run. Its hash was
unchanged after that run:

```text
de1dafb9146624a009ac65804838aced2364615031fe8fd3e1f4f0365287bf88
```

`source.sha256` records the implementation, deployment config and real-process
fixture. `admission.txt` and `http.txt` retain the focused final transcripts.
The explicitly ignored dense archive probe still fails under the unchanged
250,000-row policy; this report does not count it as a passing test.

## Production observation and remaining capacity work

A read-only `/proc/654/status` check identified `allsource-core`, reporting
2,179,580 KiB current RSS, 2,223,440 KiB high-water RSS and three threads.
The process was not restarted or changed. Current RSS is approximately 2.08 GiB,
so the Linux capacity experiment must also cover 2.5 GiB of touched background
memory rather than relying solely on a 2 GiB reservation.

The Linux release capacity probe is still building locally. Its exported source
predates this worker setting and measures one request against the original pool;
it can establish the cost of that single load, not concurrent admission behavior.
The new setting's admission behavior is covered separately above. Resource-limit,
OOM and actual Linux load results remain required before changing row admission.
No production memory headroom claim follows from the native measurement alone.
