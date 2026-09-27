# Query Service query-admission client proof

Builds on signed Core commit `adfcc8161bc4e1b0ff93df1154063d406aa91d6b`.
Related tasks: `t-7ce02d`, `t-7b4430`. Source hashes pin the client increment;
the design and remaining integration work are in
`docs/plans/2026-09-27-query-admission-client.md`.

## Result and evidence

The new internal port reads canonical snapshots and admits exact operations
through Core's leader. It validates every receipt identity field and rejects
expired, malformed, oversized, missing or legacy responses. It performs no
implicit retry, redirects, follower fallback, legacy usage increment or reset.

| Check | Result | Transcript |
|---|---|---|
| Warnings-as-errors compile | Passed | `compile.txt` (empty successful incremental output) |
| Strict Credo | Passed, 300 files | `credo.txt` |
| Full Query Service suite | 6 doctests, 1,122 tests, 0 failures, 2 skipped, 136 excluded | `query-service-tests.txt` |
| Dialyzer | Passed with existing filters unchanged: 7 filtered warnings, 1 unused filter | `dialyzer.txt` |
| Focused HTTP/restart suite | 33 tests, 0 failures, seed 342434 | `http-restart-tests.txt` |
| Explicit-file formatting and whitespace | Passed | Commands below |

Ten client checks exercise real loopback HTTP fixtures: correct leader authority,
receipt substitution, strict protocol fields, expiry during transport, malformed
snapshots, status/code matching, redirect/follower/automatic-retry refusal,
streamed response bounds, invalid input and service URL validation. Expiry at
response time uses a deterministic domain check, without changing the clock.

Five real-Core meter cases include the newly connected client: it observes an
unmanaged zero counter, admits the final unit, survives Core SIGKILL/restart,
receives the exact original receipt without charging twice, observes the managed
counter, and rejects a new operation or changed period. Eighteen existing actual
Core regressions cover run ordering, recording/evidence, strict and conditional
archive reads, and customer evidence reviews.

The full suite initially revealed interference from an older suite's buffered
`t-demo` usage flushes. The fixture acknowledges only that unrelated demo-meter
traffic, including prefixed paths while testing invalid service URLs. Requests
for this test's own tenant remain captured, including forbidden legacy fallback.
No runtime behavior was changed to make the tests pass.

Core binary is the frozen enterprise + analytics runtime from the preceding
proof; no Core behavior changed in this client increment:

```
/private/tmp/allsource-query-admission-core-JlQWNk/allsource-core
sha256 c4623ef235a683758d3abea8629bc0c5b4a33a9343204b2c944d77c1dcb84d64
```

Commands, from `apps/query-service`, each completed within a bounded timeout:

```
MIX_ENV=test mix compile --warnings-as-errors
MIX_ENV=test mix credo --strict
MIX_ENV=test mix test --seed 342434
MIX_ENV=test mix dialyzer --format short
MIX_ENV=test mix format --check-formatted \
  lib/query_service_ex/domain/customer_agent/query_admission.ex \
  lib/query_service_ex/domain/customer_agent/query_usage_port.ex \
  lib/query_service_ex/infrastructure/adapters/customer_query_usage_store.ex \
  test/query_service_ex/infrastructure/adapters/customer_query_usage_store_test.exs \
  test/query_service_ex/integration/query_usage_admission_test.exs
ALLSOURCE_CORE_BINARY=<frozen-binary> MIX_ENV=test mix test --include integration --seed 342434 \
  test/query_service_ex/infrastructure/adapters/customer_query_usage_store_test.exs \
  test/query_service_ex/integration/query_usage_admission_test.exs \
  test/query_service_ex/integration/agent_run_ordering_test.exs \
  test/query_service_ex/integration/agent_run_recorder_test.exs \
  test/query_service_ex/integration/agent_run_evidence_test.exs \
  test/query_service_ex/integration/conditional_archive_integrity_test.exs \
  test/query_service_ex/integration/strict_retained_read_test.exs \
  test/query_service_ex/integration/customer_evidence_review_test.exs
git diff --check
```

## Limits

The port is not yet called by customer application services. Stable operation
persistence, admission before source reads, eligibility after consuming the final
unit, multi-instance work limits, cancellation, billing reset adoption, MCP
preparation/status/result binding, human product gate/display, skills and actual
host proof remain open. No access/tool acceptance criterion is closed here.

All data and credentials used here are synthetic. Customer flags stay off;
held bets remain held. Neither this client nor the preceding Core meter is
deployed. Exact-source CI and deployment approval/rollout validation remain
required. Automatic review previously rejected Fly source upload/registry push
without destination-specific approval; no retry or bypass was attempted here.
