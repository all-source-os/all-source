# Customer review work bounds

Continuation of `t-7ce02d` and `t-7b4430`, based on
`06d9f0f72977663076c368ce7167a9862188c366`.
Design: `docs/plans/2026-09-27-customer-review-work-bounds-design.md`.

## Behavior and scope

Source sharing, comparison preparation and review/result reads now use a
dedicated supervised workflow pool: four active jobs per Query Service instance,
two per tenant and a 20-second admitted-work deadline. Excess work receives a
fixed busy error immediately, before quota reservation or source access.

Caller exit or deadline cancels the worker. Its capacity remains occupied until
the worker exits. The task supervisor and dispatcher form one restart unit;
restarting either removes old workers before replacement admission. Worker
errors and dispatcher status formatting do not expose inputs or result bodies.
Busy callers can retry the same request later. Canceled admitted work can have
already committed usage; the existing journal prevents another charge.

All retained source reads use the fixed Core leader's existing archive pool,
including warm scans and cold preparation. That database pool holds its permit
through canceled HTTP callers until blocking work finishes. This increment adds
no lease database and changes no Core runtime behavior. The gateway limit is
per instance; it is not a cluster-wide limit of four whole workflows.

## Verification

Initial focused run: 29 tests, zero failures, seed 42017. This combines ten new
supervision/cancellation cases, two actual-Core workflow cases, existing quota
and retry cases, pending-review recovery and strict-retained-read regressions.

The actual Core cases prove:

- A saturated tenant performs no source read or query charge. After capacity
  returns, the unchanged request succeeds on its final available quota unit.
- After a real retained source read, killing the caller kills its paused worker;
  no source record appears afterward. Retrying the original request creates one
  source record while canonical usage remains one.
- Existing concurrent preparation now accepts explicit busy responses; later
  retries keep one exact pending review and one comparison charge.

The original Core worker-pool cancellation and single-worker HTTP evidence is
recorded in `../2026-09-27-archive-worker-setting/README.md`. Capacity and row
policy evidence is in `../2026-09-27-isolated-archive-capacity/README.md`. Those
are separate prior checks, not tests newly rerun by this Query Service change.

The additional real crash-log test initially failed: state and last-message
redaction did not remove the original exception stack arguments. The installed
OTP 28.1.1 `gen_server` implementation appends that stack independently of
`format_status/1`. Dispatcher callbacks now contain exceptions and stop with a
fixed reason; the same real failure scenario passes without the synthetic
private markers. `privacy-before.txt` retains the failing synthetic observation;
`supervision-final.txt` records all eleven supervision/privacy cases passing.

Final verification, seed 42017:

| Gate | Result | Evidence |
|---|---|---|
| Compile with warnings denied | Passed | `compile.txt` |
| Strict Credo | Passed, 312 files | `credo.txt` |
| Full Query Service suite | 6 doctests, 1,137 tests, zero failures, 2 skipped, 147 excluded | `query-service-tests.txt` |
| Dialyzer | Passed, 7 existing warnings filtered and 1 unused filter; filters unchanged | `dialyzer.txt` |
| Final supervision/privacy suite | 11 tests, zero failures | `supervision-final.txt` |
| Final focused and actual-Core workflow suite | 30 tests, zero failures | `http-cancellation-tests.txt` |
| Explicit-file formatting and whitespace | Passed | Commands below |

All gates were rerun after containing callback failures. The final source comment
explains that fix without changing behavior. `source.sha256` pins implementation,
tests and documentation. `core-binary.sha256` identifies the unchanged frozen
enterprise + analytics Core binary used for real HTTP and recovery checks.

Commands, from `apps/query-service`, each executed with a bounded timeout:

```text
MIX_ENV=test mix format --check-formatted <changed Elixir files in source.sha256>
MIX_ENV=test mix compile --warnings-as-errors
MIX_ENV=test mix credo --strict
MIX_ENV=test mix test --seed 42017
MIX_ENV=test mix dialyzer --format short
MIX_ENV=test mix test --seed 42017 test/query_service_ex/application/services/customer_review_work_test.exs
ALLSOURCE_CORE_BINARY=/private/tmp/allsource-query-admission-core-JlQWNk/allsource-core MIX_ENV=test mix test --include integration --seed 42017 \
  test/query_service_ex/application/services/customer_review_work_test.exs \
  test/query_service_ex/integration/customer_review_work_test.exs \
  test/query_service_ex/integration/metered_evidence_workflow_test.exs \
  test/query_service_ex/integration/customer_evidence_review_test.exs \
  test/query_service_ex/integration/strict_retained_read_test.exs
git diff --check
```

## Remaining delivery

No customer feature is enabled and no production deployment occurs here.
Billing-period transition adoption, public source/preparation/status/result
bindings, human-only product approval, display, complete customer skill/host
journeys and rollout proof remain required. Synthetic local proof does not
complete any access/tool acceptance criterion or establish customer outcomes.

Core admission candidate `adfcc816` has its own exact-source CI and pending
destination-specific Fly authorization. This Query Service change does not alter
that candidate. Existing unrelated Prime, web analytics and outreach edits are
excluded. Prime design indexing failed because its current store is a read-only
replica; no competing writer was stopped.
