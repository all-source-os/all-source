# Internal customer evidence review verification

Date: 27 September 2026. Base: `a49d8c43359b99f198d8e814a7b0c80acf8ebb63`.
Scope: existing source-access/preparation work. No deployment, host upload,
customer data processing, public route or consequential action.

## Verified behavior

New domain tests first failed with the missing implementation. Current checks
cover owner/tenant/host/grant pins, exact source revisions/digests, expiry,
clock rollback, unknown/private field denial, separately versioned consent,
pending-only state, digest changes and bounded rolling issuance.

Actual Core integration uses private synthetic WAL directories, loopback HTTP
and the existing Core binary. It exercises real scoped grants and current stored
membership/entitlement, two actual typed runs, explicit synthetic source consent,
source creation, proposal preparation, reading and restart recovery. Tests prove:

- Exact preparation retries recover one pending record and unchanged view after
  SIGKILL/restart, preserving the original digest and expiry.
- Four concurrent preparations with one key produce one identical pending record.
- Changed preparation content under the same key is refused.
- A simulated lost response after a real config write recovers without a second
  write; it never becomes implicit approval.
- Another connection, owner, tenant, old consent and revoked membership cannot
  retrieve the private review.
- Source changes supersede a pinned view; expiry hides its content.
- Independent deletion markers remain effective after stale workspace restoration
  and Core restart. Deleted review retrieval is refused.
- Stored review metadata contains opaque references rather than copied run/event
  payloads or connection credentials. Report construction performs no execution.

Actual HTTP fixtures verify fixed key/path/authority binding, required conditional
revision, body-size limit, redirect refusal, no implicit retries, invalid-input
denial and fixed upstream errors. Existing connection and run-adapter regressions
run alongside these tests. Browser fixture tests remain optional and are not
counted as host proof.

Full Query Service suite passed: **6 doctests, 1,107 tests, zero failures,
two skipped, 126 integration tests excluded**. Explicit Core tests are separate.
The first full run exposed unrelated buffered UsageReporter traffic reaching
run-source HTTP fixtures through the shared test Core URL. Fixtures now separate
that background billing path; production metering code is unchanged.

Final focused/static checks passed:

| Check | Result |
| --- | --- |
| Combined new domain/store/Core review tests, existing connections and run HTTP adapter | 35 tests, zero failures, one optional browser fixture skipped |
| `mix format --check-formatted` | Passed |
| `mix compile --warnings-as-errors` | Passed |
| `mix credo --strict` | Passed |
| `mix dialyzer --format short` | Passed; seven existing filtered warnings and one unnecessary filter, unchanged |
| Tenant isolation architecture gate | Passed; seven existing documented PubSub exceptions unchanged |
| `git diff --check` | Passed |

All commands use
`gtimeout -k 15 300`; Mix runs in `apps/query-service`, and explicit integration
sets `ALLSOURCE_CORE_BINARY` to the repository's built `target/debug/allsource-core`.

## Limits

This proves internal services, not an end-to-end customer product flow. The
source-selection service receives an already-authenticated product actor; no
new browser transport proves that actor here. Public MCP discovery still has two
eligibility/validation tools. Source/preparation transport, metering, consent and
review UI, human receipt/execution, native and hosted Claude tests, and deployment
remain open. Ordinary ingestion and existing commercial rules are unchanged.

Only two-run comparison preparation is implemented here. Generic event timelines,
restart evidence, replay plans, draft edits and accepted results remain required.
Deletion tests prove deny markers, not physical erasure or full-backup rollback
protection. Synthetic lost-response tests discard a real successful store reply;
they do not emulate every network partition. Source observations are independent,
not an atomic multi-run/permission snapshot.

See the [contract](../../plans/2026-09-27-customer-evidence-reviews.md). Source and
artifact hashes identify local tested code and Core binary, not a hosted image.
Unrelated edits are excluded. No acceptance item is closed by this record alone.
