# Metered customer evidence workflow proof

Continuation of `t-7ce02d` and `t-7b4430`, based on
`dc55548f7fc068eac428fdd6244c326eb0dba87d`. Source hashes pin this increment.
Design: `docs/plans/2026-09-27-metered-evidence-workflow-design.md`.

## Verified behavior

Source sharing and pending review preparation/read now use canonical Core query
admission before source work. A bounded conditional Core record retains the
original count, fingerprint and billing generation. A one-hour timestamped
request ID cannot be revived after pruning; the derived metering ID binds owner,
grant, host and purpose. No second billing counter or database is introduced.

Source sharing consumes one planned query attempt; comparison preparation and
live evidence reads consume two. Reusing the exact request does not charge again,
including after an uncertain reply or hard Core restart. A later reset cannot
silently move a retry into a new period. Current identity, membership, consent,
source and subscription checks still apply after the final quota unit is spent.

Quota refusal, changed intent, unsupported/incomplete proposals, unavailable
operation storage and revoked source retries perform no source read. Revocation
during admitted source work prevents disclosure. Shared references are checked
again against current durable records before return.

## Verification

| Gate | Result | Evidence |
|---|---|---|
| Warnings-as-errors compile | Passed | `compile.txt` (empty successful incremental output) |
| Strict Credo | Passed, 308 files | `credo.txt` |
| Full Query Service suite | 6 doctests, 1,126 tests, 0 failures, 2 skipped, 145 excluded | `query-service-tests.txt` |
| Dialyzer | Passed; seven existing warnings filtered, one unused filter; configuration unchanged | `dialyzer.txt` |
| Focused domain/client/actual Core regressions | 60 tests, 0 failures, seed 342434 | `http-restart-tests.txt` |
| Added concurrent-charge assertions | Affected six actual Core tests rerun: 0 failures | `concurrency-final.txt` |
| Explicit-file formatting and whitespace | Passed after final edits | Commands below |

The combined 60-case run includes nine new actual workflow scenarios, the six
existing pending-review recovery/isolation cases, the canonical-meter client and
HTTP cases, and run/archive regressions. Unit cases cover the 192-live-operation
journal cap, pruning/expiry, immutable identity, owner/host binding, malformed
state, clock reversal and metered eligibility after quota exhaustion.

Actual workflow tests observe every source read through a pass-through adapter;
they do not substitute in-memory source data or billing counters. Tests verify
one-unit sharing, two-unit preparation/read, final-unit completion, lost journal
and admission replies, hard restart, changed-period refusal, changed intent,
revoked membership/source, inaccessible journal and unsupported preparation.

After the combined run, two assertions were added to the existing concurrency
case: four simultaneous preparation retries leave exactly four canonical query
units used (two source shares plus one comparison), and reversed intent does not
change that counter. The entire affected six-case file passed again. No runtime
behavior changed after the combined run.

Full-suite execution exposed an unrelated legacy reporter flushing `t-demo-q`
into the query-client fixture. It now acknowledges legacy increments for other
tenants while retaining all requests for the tested tenant, including forbidden
fallback, plus the tested redirect target. No runtime reporter behavior was changed.

Core was the existing frozen enterprise + analytics binary; this increment
changes Query Service and its internal contract, not Core:

```
/private/tmp/allsource-query-admission-core-JlQWNk/allsource-core
sha256 c4623ef235a683758d3abea8629bc0c5b4a33a9343204b2c944d77c1dcb84d64
```

Commands from `apps/query-service`, executed under bounded timeouts:

```
MIX_ENV=test mix compile --warnings-as-errors
MIX_ENV=test mix credo --strict
MIX_ENV=test mix test --seed 342434
MIX_ENV=test mix dialyzer --format short
MIX_ENV=test mix format --check-formatted <changed Elixir files listed in source.sha256>
ALLSOURCE_CORE_BINARY=<frozen-binary> MIX_ENV=test mix test --include integration --seed 342434 \
  test/query_service_ex/domain/customer_review_workspace_test.exs \
  test/query_service_ex/domain/customer_agent/eligibility_test.exs \
  test/query_service_ex/domain/customer_agent/query_operation_journal_test.exs \
  test/query_service_ex/infrastructure/adapters/customer_query_usage_store_test.exs \
  test/query_service_ex/integration/metered_evidence_workflow_test.exs \
  test/query_service_ex/integration/query_usage_admission_test.exs \
  test/query_service_ex/integration/agent_run_ordering_test.exs \
  test/query_service_ex/integration/agent_run_recorder_test.exs \
  test/query_service_ex/integration/agent_run_evidence_test.exs \
  test/query_service_ex/integration/conditional_archive_integrity_test.exs \
  test/query_service_ex/integration/strict_retained_read_test.exs \
  test/query_service_ex/integration/customer_evidence_review_test.exs
ALLSOURCE_CORE_BINARY=<frozen-binary> MIX_ENV=test mix test --include integration --seed 342434 \
  test/query_service_ex/integration/customer_evidence_review_test.exs
git diff --check
```

## Exact limits

- These are internal application services, not discovered MCP preparation,
  status or result tools. Existing context/validation bindings stay unchanged.
  No public customer source endpoint, credential or feature flag is enabled.
- The timestamped request shape updates previously unbound internal interfaces.
  Stored source/review record fields remain compatible. Product and MCP bindings
  must preserve request identities; they may not manufacture retries or periods.
- Admission charges a planned query attempt even if later storage/source work
  fails. It grants no source consent, human approval, refund or execution.
- Work bounds across service replicas, cancellation proof and actual billing
  reset adoption remain required before customer activation. Existing generic
  read/ingestion metering behavior is unchanged.
- Product source consent, human approval/display, MCP bindings, full skill and
  actual host journeys remain open. No access/tool acceptance criterion closes
  from this proof alone. Held bets remain held.
- Proof is synthetic and local. This does not prove production capacity,
  failover or customer outcomes. Exact-source CI, deployment authorization and
  rollout validation are still required; no production update occurred here.
- Prime design indexing was attempted but its server is a read-only replica.
  The versioned plan remains available; no competing writer was stopped.
