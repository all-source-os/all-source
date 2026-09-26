# Scoped grant primitive evidence — 26 September 2026

Historical evidence for the initial tenant-metadata implementation. That storage
design was superseded after reproducing stale billing writes restoring revoked
access. See [the isolation fix and real-Core recovery evidence](../2026-09-26-customer-agent-grant-isolation/README.md).
The results below describe the original source bytes, not the replacement store.

Progress only: `t-7ce02d` remains open. Base: signed `ad4ab816`.
[Implemented boundary and missing runtime work](../../plans/2026-09-26-customer-agent-grants.md).
[Tested source bytes](source-sha256.txt).

From `apps/query-service`, local Elixir 1.19.1 / OTP 28.1.1; all commands used
`MIX_ENV=test` and `gtimeout -k 15 300`.

| Check | Observed result |
|---|---|
| `mix compile --warnings-as-errors` | Exit 0. |
| `mix format --check-formatted` | Exit 0. |
| `mix credo --strict` | Exit 0, 228 files, no issues. [Output](credo.txt). |
| `mix dialyzer --format short` | Exit 0; seven existing suppressed findings and one unused filter; no filter changed. [Output](dialyzer.txt). |
| Targeted grant-store test file | Exit 0, 8 tests. Original five cases failed before implementation. |
| `mix test` | Exit 0, 6 doctests, 1,033 tests, 0 failures, 2 skipped, 91 excluded. [Summary](tests-summary.txt). |

Full run also used CI's `TESTCONTAINERS_RYUK_DISABLED=true`. Raw local outputs:
`/private/tmp/allsource-grant-red-20260926.log`,
`/private/tmp/allsource-grant-targeted-20260926.log`,
`/private/tmp/allsource-grants-final-suite-20260926.log`.
Only test summaries are committed; unrelated request/log content is omitted.

## Verified through actual HTTP adapter calls

The test starts an ephemeral loopback Bandit Core fixture and exercises the real
`RustCoreClient` network adapter, JSON metadata merge and new credential store:

- Issuance persists only a credential hash and preserves unrelated tenant quota
  metadata. Each verification performs a new leader request.
- Revocation is idempotent; a separate caller subsequently fails with the same
  token, demonstrating no process-local credential cache.
- Tenant, subject, client, exact resource, operation, clock and deadline bind the
  credential. Tampered secrets, missing grants and invalid path input deny use.
- A configured healthy-but-unreachable follower cannot intercept authorisation
  reads; issue/verify/revoke continue through the fixture leader.
- A 503 leader response produces one request and a fixed unavailable error,
  without retry or upstream response-body disclosure.
- Failed durable writes return failure, never a working token or successful
  revocation. Opaque review token fails the existing API-key JWT verifier.

## Limits

Fixture is a synthetic in-memory HTTP server, not a running Core WAL. These
checks do not prove actual Core restart/failover, production credentials, host
installation, entitlement, membership, processing consent, customer outcome or
end-to-end MCP access. No endpoint accepts the new credentials yet. Grant-store
success must not be interpreted as data access or human approval.

Issuance/revocation are internal primitives requiring server-authorised context.
Per-tenant grant-count/rate controls and audit-retention policy remain missing;
do not expose issuance before those controls and live eligibility are wired.
Existing generic API-key revocation behaviour is not changed by this work.
Unrelated pre-existing files were preserved. Nothing deployed or activated.
