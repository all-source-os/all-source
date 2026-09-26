# Customer agent contract evidence — 26 September 2026

Task: `t-baaca8`. Base: `f8dbf3c8` on `main`.
Implementation contract: [design and release boundaries](../../plans/2026-09-26-customer-agent-contract.md).
Tested source bytes: [SHA-256 manifest](source-sha256.txt).

## Results

All commands ran from `apps/query-service` with `MIX_ENV=test` under
`gtimeout -k 15 300`. Local toolchain: Elixir 1.19.1 / OTP 28.1.1. CI uses Elixir
1.18 / OTP 27, so local success is not a claim of an observed CI run.

| Command | Result |
|---|---|
| `mix deps.get` | Installed existing locked dependencies; `mix.lock` unchanged. |
| `mix deps.unlock --check-unused` | Exit 0. |
| `mix format --check-formatted` | Exit 0 across Query Service. |
| `mix compile --warnings-as-errors` | Exit 0. |
| `mix credo --strict` | Exit 0; 225 files, no issues. [Output](credo.txt). |
| `mix dialyzer --format short` | Exit 0; 7 findings covered by existing filters, no new filters. [Output](dialyzer-gate.txt). |
| `mix dialyzer --list-unused-filters --format short` | Diagnostic exit 1: existing `core_websocket_worker.ex.*call_with_opaque` filter unused on this toolchain. CI explicitly treats this diagnostic as non-blocking; filter unchanged. [Output](dialyzer-diagnostic.txt). |
| `mix test` | Exit 0: 6 doctests, 1,025 tests, 0 failures, 2 skipped, 91 integration exclusions. [Summary](tests-summary.txt). |

The full suite used CI's `TESTCONTAINERS_RYUK_DISABLED=true` and repository test
helper's existing `:integration` exclusion. It does not prove a live Core,
subscription provider, tenant or MCP host. Full local stdout remains at
`/private/tmp/allsource-contract-final-suite-20260926.log`; the checked-in summary
omits unrelated request/log content. No test suppression was added.

Before implementation, all 15 initial new tests failed with missing modules
(`/private/tmp/allsource-contract-red-20260926.log`). After implementation, the
15 new cases plus five existing replay controller cases passed. A subsequent
coverage-consistency regression increased the new cases to 16, included in the
final full run above. Initial sandbox test invocation failed before execution
because Mix PubSub could not open its local socket; permitted rerun installed
locked dependencies and exercised the actual suite.

## What the evidence establishes

- Strict request and source shapes; rejected authority/query/payload fields,
  invalid revisions/digests, duplicate handles, oversize input, unknown kinds and
  incompatible targets; partial evidence remains unknown.
- Proposal fingerprint binds source revision, digest, projection and comparison
  direction. Baseline/candidate reversal changes the digest.
- Pending records bind server-supplied owner and expiry. Changed sources
  supersede pending/approved records; exact deadline expires them. Rejection is
  terminal; unknown authority or clock rollback returns unavailable.
- Replay snapshot uses the existing catalog and strips entity IDs, event names,
  arbitrary text and readiness-as-permission. Counts and scope cannot contradict
  each other; absent facts are null.
- Existing replay controller test invokes real `ReplayAnalysis.analyze/2`, passes
  its result through the new snapshot, checks matching counts and verifies that
  no replay was started. Synthetic Core responses only.

## Limits

No new route, tool, grant store, human approval endpoint, renderer, host package
or deployment exists from this change. The pure review status type cannot
authenticate a persisted decision and must never authorise execution by itself.
Live source resolution, revocation, current entitlement, browser authority,
durable one-use receipts, deletion, actual host proofs and rollout remain in the
named dependent beads. Underlying run comparison/restart work remains owned by
`t-e3d99f`. This is completed schema/boundary work, not a completed customer flow.

Unrelated pre-existing Prime metadata, web analytics, outreach and manual-action
documentation were preserved and are outside this commit.
