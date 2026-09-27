# Internal typed run capture verification

Date: 27 September 2026. Base: `dcf0967884d105467bfe3ea512d08750ad4ef0a3`.
Owner: existing epic `t-e3d99f`. No new public route, deployment, customer data
disclosure, SDK execution wrapper or host/model call.

## Implemented behavior

The recorder validates tenant/run binding, exact input fields, retained history,
expected version, causation and lifecycle before a single conditional Core write.
It verifies the response against stored evidence. Exact retries recover the same
event acknowledgement; changed operation content fails. Uncertain writes remain
unknown and are never retried automatically. No external action or human
authority follows from an acknowledgement. See the
[contract](../../plans/2026-09-27-agent-run-evidence-contract.md#typed-capture-and-retry-recovery).

## Tests and evidence

Five new domain tests initially failed because the append implementation was
absent, then passed with the implementation. Domain and service tests cover
wrong tenant/run, unknown/private fields, invalid version, stale causation,
invalid transitions, corrupt history, operation scoping, changed retry content,
unavailable source, uncertain write and success-shaped but absent evidence.

Four actual-Core integration tests use the existing built Core binary, private
synthetic WAL directories and loopback HTTP. They verify:

- A full seven-event run and exact retry acknowledgements survive SIGKILL and
  restart; retries do not add events or change the reconstructed timeline.
- Eight concurrent identical requests store exactly one event and share its ID.
- Competing different requests produce exactly one winner for the same version.
- A simulated lost acknowledgement after a real Core append remains uncertain;
  after hard restart, read recovery returns the existing start without another
  append. The unresolved attempt prevents another attempt from starting.

The lost-response fixture deliberately discards the actual successful adapter
result. It does not claim to emulate every network partition or failover.

Actual HTTP adapter fixtures check fixed endpoint, authority/header binding,
closed metadata, no retry, 4 KiB acknowledgement cap, response-version mismatch,
fixed failure errors, conflict handling and denial before network access. Existing
read tests retain the 2 MiB cap and partial-history rejection.

Full Query Service suite: **6 doctests, 1,093 tests, zero failures, two skipped,
120 integration tests excluded**. Explicit actual-Core tests run separately.
Final focused checks and static analysis are recorded below.

Final checks passed with `gtimeout -k 15 300`:

| Command/check | Result |
| --- | --- |
| Explicit run domain, service, HTTP adapter and actual-Core suite, `mix test --include integration` | 43 passed, zero skipped |
| `mix format --check-formatted` | Passed |
| `mix compile --warnings-as-errors` | Passed |
| `mix credo --strict` | Passed |
| `mix dialyzer --format short` | Passed; seven existing filtered warnings and one unnecessary filter, unchanged |
| `cargo run --manifest-path tooling/tenant-isolation-check/Cargo.toml` | Passed; seven existing documented PubSub exceptions unchanged |
| `git diff --check` | Passed |

Mix commands ran in `apps/query-service`. Explicit integration checks set
`ALLSOURCE_CORE_BINARY` to the repository's built `target/debug/allsource-core`.
The full suite preceded a control-flow-only Credo correction; the final 43-test
suite covers the corrected recorder. No Core source changed after the previously
verified all-feature build. No web, SDK or MCP surface changed in this increment.

## Limits

This internal service is not authentication, entitlement, metering, source consent,
product approval, SDK capture or customer delivery. Current review grants retain
their eligibility/validation-only scope. Ordinary SDK ingestion stays unchanged.
The epic and dependent customer-delivery tasks remain open.

Recovery requires complete retained history. No indefinite idempotency guarantee
applies after expiry/deletion, and old run IDs must not be reused. Generic writers
with the same tenant authority can supply metadata; this is not cryptographic
attestation. No external effect is executed, inferred or retried. Core failover,
retention high-water semantics and cache-eviction races remain outside this proof.

Source hashes identify this tested revision. The Core artifact hash identifies
the local binary, not a production image. Foreign worktree edits are excluded.
