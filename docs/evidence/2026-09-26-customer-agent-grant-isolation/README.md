# Customer grant isolation and Core recovery — 26 September 2026

Progress on `t-7ce02d`, not a completed or deployed customer MCP connection.
Base source: signed `00f70228b60de03d93787e37f3f6359bf7b7b2f3`.
[Current boundary and remaining access work](../../plans/2026-09-26-customer-agent-grants.md).
[Tested source hashes](source-sha256.txt).

## Reproduced failure and correction

The initial implementation stored active grants inside tenant metadata.
Control Plane's `UpdateSubscriptionMetadataUseCase` reads and replaces a complete
metadata map. A delayed billing write could therefore restore a revoked grant.
The HTTP-adapter regression reproduced this: 9 tests, 1 failure; verification
returned a valid active grant where the test required `:unauthorized`.
Raw local output: `/private/tmp/allsource-grant-resurrection-red-20260926.log`.

Grants now use existing admin-only Core system config records. Revocation uses
an independent key. Tenant writes cannot touch either record, and rewriting an
older grant cannot remove the revocation marker. No Core source or schema was
changed. This does not solve the separate billing metadata concurrency issue
tracked by `t-cad0f0`.

## Observed checks

Query Service commands ran from `apps/query-service` with `MIX_ENV=test` and
`gtimeout -k 15 300`, using local Elixir 1.19.1 / OTP 28.1.1. CI uses
Elixir 1.18 / OTP 27; this is local verification, not a CI run.

| Check | Observed result |
|---|---|
| `mix format --check-formatted` | Exit 0. |
| `mix compile --warnings-as-errors` | Exit 0. |
| `mix credo --strict` | Exit 0; 229 files, no issues. [Output](credo.txt). |
| `mix dialyzer --format short` | Exit 0; seven existing suppressed findings and one unused filter. No filters changed. [Output](dialyzer.txt). |
| Two grant test files, `mix test --include integration` with `ALLSOURCE_CORE_BINARY` | Exit 0; 13 tests, 0 failures: 12 HTTP-fixture cases and one real-Core recovery scenario. [Summary](tests-summary.txt). |
| Full `mix test`, with `TESTCONTAINERS_RYUK_DISABLED=true` | Exit 0; 6 doctests, 1,037 tests, 0 failures, 2 skipped, 92 excluded. Integration tests are excluded by default. [Summary](tests-summary.txt). |

The first strict-Credo run found excessive complexity in the HTTP fixture; its
config-read handling was extracted and the final gate passed. Dialyzer ran on
the final production modules; only test fixtures and documentation changed
afterward. Full logs remain in `/private/tmp/allsource-grant-isolation-*-20260926.log`.
Committed outputs omit unrelated request/log content.

## Real Core crash-recovery scenario

Built from repository root, without source changes:

```text
gtimeout -k 15 300 cargo build --offline -p allsource-core --bin allsource-core --features enterprise
```

Build exited 0. [Binary hash and build context](core-binary.txt). The first
attempt used the default community build and failed at tenant creation with
HTTP 404; tenant routes require `multi-tenant`, supplied by `enterprise`.
That attempt is not counted as successful recovery proof.

The opt-in ExUnit scenario starts the actual binary on loopback with a private
temporary directory, synthetic signing secret, authentication enabled,
replication disabled and no bootstrap key. It makes real HTTP calls:

1. Create a synthetic tenant, issue and verify a grant. A developer credential
   cannot read the grant or write its revocation marker. Kill the owned Core
   process with SIGKILL and await exit.
2. Restart against the same system WAL; verify the issued credential. Revoke it,
   then PUT the earlier complete tenant metadata and rewrite the original grant
   record. Verification denies access. SIGKILL and await exit again.
3. Restart against the same WAL; revoked access remains denied and repeated
   revocation succeeds. Kill the owned process and remove temporary data.

Startup, HTTP requests and shutdown are bounded. Tests use no production
credentials or customer data. The test talks to the compiled Core binary over
HTTP; Query Service does not import Core source.

The HTTP-fixture suite additionally verifies exact bindings, expiry, tampered
secrets, unavailable leader, no retry, stale-follower avoidance, failed writes,
generic JWT rejection and fixed errors. An unavailable revocation lookup denies
access even when the grant lookup succeeds. Any existing marker denies access,
including null, false and malformed marker values.

## Limits and release status

Single-node local WAL recovery is proved; replication, failover, production
topology and actual customer host connection are not. This is credential
verification, not live membership, paid entitlement, source ownership, consent
or human authority. The service's Core administrator remains trusted.

No route or MCP tool exposes these primitives. Issuance/revocation need verified
server authority, current eligibility, size/rate/concurrency/grant-count limits,
retention policy and actual transport binding before exposure. Result disclosure
must recheck authorization; the two credential reads are not a transaction with
later data retrieval. Human actions still require the separate product gate.

All `t-7ce02d` acceptance criteria remain unchecked. No customer credential was
minted in production, no source data retrieved, no deployment or pilot activated.
