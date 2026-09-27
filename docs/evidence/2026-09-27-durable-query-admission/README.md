# Durable canonical query admission

Dependency of `t-7ce02d` and `t-7b4430`, built on
`4a0c039205097e948042ad6f3c9619004d401704`. The source manifest pins this increment.
This is an internal Core primitive, not a completed customer MCP feature.

## Result

One fsynced tenant update stores the canonical `quotas.queries_used` counter and
an exact retry receipt together. Same-operation retries do not charge again;
concurrent callers cannot both consume the final quota unit. All tenant writers
share the same lock, and generic metadata writes preserve a managed query meter.
A separate monotonic generation makes resets retry-safe without changing the
shared `reset_date`, event usage, x402 usage or extraction usage.

Proof uses synthetic tenants, owned loopback processes and temporary WALs. No
production credentials, sources, writes or customer feature flags were used.

## Verification

| Check | Result | Evidence |
|---|---|---|
| Core all-features tests | 2,360 passed, 0 failed, 14 ignored across 48 suites | `core-tests.txt` |
| Core strict Clippy | Passed, existing dependency future-compat warnings only | `clippy.txt` |
| Query Service strict Credo | Passed, 296 files | `credo.txt` |
| Actual Core HTTP and hard-restart regressions | 22 passed, 0 failed, seed 342434 | `http-restart-tests.txt` |
| Enterprise + analytics runtime build | Passed | `runtime-build.txt` |
| Rust/Elixir formatting and whitespace | Read-only checks passed | Commands below |

The eight new Rust integration cases cover exact receipt recovery, final-unit
races, stale metadata, independent resets, malformed/expired requests, legacy
increments sharing the counter, all 4,096 receipt slots and invalid canonical
quotas. A private unit test queues an already-admitted retry behind the tenant
lock until expiry and verifies refusal after the lock becomes available.

Four new HTTP cases exercise those routes through real authentication and
SIGKILL/reopen: lost acknowledgement, eight simultaneous final-unit callers,
stale PUT/PATCH and reset ordering, and non-admin/malformed/oversized refusals.
The remaining 18 cases cover existing run ordering/recording/evidence,
conditional archive integrity, strict retained reads and customer evidence.

Actual HTTP proof exposed a test assumption: existing Core middleware returns
401 for every non-admin tenant-management path, including opaque metadata
patches. Tests now assert that existing boundary; this change does not broaden
direct customer metadata access. The added handler-level billing guard is
defence in depth, not a separately proven customer PATCH route.

Native runtime was frozen after the Core suite and enterprise build:

```
/private/tmp/allsource-query-admission-core-JlQWNk/allsource-core
sha256 c4623ef235a683758d3abea8629bc0c5b4a33a9343204b2c944d77c1dcb84d64
```

Only documentation comments and test expectations changed after that freeze;
runtime behavior is the tested implementation. No current proof relies on the
earlier discarded reset-date-coupled prototype. Initial sandbox failures occurred
before tests ran; the final HTTP suite used approved local socket execution.

Commands from the repository root unless noted:

```
cargo test -p allsource-core --all-features --tests
cargo clippy -p allsource-core --all-features --all-targets -- -D warnings
cargo fmt --all --check
git diff --check
RUSTC_WRAPPER= cargo build -p allsource-core --bin allsource-core --features enterprise,analytics
# apps/query-service:
MIX_ENV=test mix credo --strict
MIX_ENV=test mix format --check-formatted test/query_service_ex/integration/query_usage_admission_test.exs
ALLSOURCE_CORE_BINARY=<frozen-binary> MIX_ENV=test mix test --include integration --seed 342434 \
  test/query_service_ex/integration/query_usage_admission_test.exs \
  test/query_service_ex/integration/agent_run_ordering_test.exs \
  test/query_service_ex/integration/agent_run_recorder_test.exs \
  test/query_service_ex/integration/agent_run_evidence_test.exs \
  test/query_service_ex/integration/conditional_archive_integrity_test.exs \
  test/query_service_ex/integration/strict_retained_read_test.exs \
  test/query_service_ex/integration/customer_evidence_review_test.exs
```

## Limits and remaining work

- Admission is administrative quota accounting, not source authority, successful
  retrieval, human approval or an execution capability.
- Its 16-request semaphore bounds metering, not subsequent source work. Query
  Service still needs stable retry identity, admission before heavy reads,
  current access rechecks and bounded work across replicas.
- Billing callers must adopt the explicit generation transition before customer
  activation. No automatic period schedule, money movement or calendar change
  is included. Unrelated legacy metadata concurrency is not globally repaired.
- Source transport, product consent/display, approval receipt, MCP operations,
  skills and actual customer-host proof remain open. No access AC is closed.
- Older Core readers retain the canonical counter but cannot reconstruct retry
  receipts. Disable customer admission before an old-server rollback.
- Local single-leader WAL recovery is proven; replication/failover and production
  capacity for this meter are not. The 14 ignored tests remain unexecuted here.
- This increment is not deployed. Exact-source CI and rollout checks remain
  required. Automatic approval review previously rejected remote Fly source
  upload/registry push without destination-specific authorization; that gate
  remains unresolved and was not bypassed.
