# Verification record

27 September 2026, local macOS, synthetic identities/data. Base commit:
`ed89f5679370174ca822e130e87560343b4e656f`.

| Check | Observed result |
|---|---|
| Full Query Service suite | 6 doctests, 1,049 tests, 0 failures, 2 skipped, 101 integration exclusions |
| Final focused HTTP adapter + actual Core + HTTP + compiled MCP | 26 tests, 0 failures, 2 intentional skips; 24 executed |
| Manual browser fixture | 4 tests, 0 failures, 74.8 seconds; includes the three actual-Core connection tests |
| Query Service warnings-as-errors compile | Passed |
| Query Service strict Credo | Passed; no suppressions added |
| Query Service Dialyzer | Passed; existing 7 filters and 1 unnecessary skip unchanged |
| Full web suite | 42 files, 200 tests passed |
| Focused web proxy and UI tests | 17 passed |
| Web TypeScript and production build | Passed; final build marks connections page dynamic |
| Targeted Biome | Passed; pre-existing literal-key style suggestions in generic v1 proxy remain informational |
| Diff whitespace check | Passed |

Full-suite results precede the final small changes restricting form issuance to
Claude Code and lowering management input to 4 KiB. The final focused run includes
both changes, actual HTTP rejection assertions and the existing compiled MCP
journey. No native Claude host ran: one focused skip is the explicitly optional
native host case, the other the opt-in manual browser fixture.

Focused command, from `apps/query-service`, with local absolute paths set for
`ALLSOURCE_CORE_BINARY` and `ALLSOURCE_CUSTOMER_MCP_BINARY`:

```text
gtimeout -k 15 300 mix test --include integration \
  test/query_service_ex/integration/customer_connections_test.exs \
  test/query_service_ex/integration/customer_agent_grant_core_test.exs \
  test/query_service_ex/integration/customer_agent_http_test.exs \
  test/query_service_ex/infrastructure/adapters/customer_agent_grant_store_test.exs
```

Actual-Core tests cover exact signed actor checks, explicit consent, scope/body
rejection, owner-only receipts/revocation, removed membership, canceled billing,
concurrent live-cap enforcement, WAL recovery, stale registry writes and revoked
MCP reconnect. HTTP fault fixtures additionally cover failed persistence,
unavailable revocation lookup, rolling-day rate limits, clock rollback and
missing consent. Browser proxy tests cover origin, cookie-only authority,
generic-proxy bypass, no-store, bounded bodies and fixed upstream errors.

Initial test failures were corrected without weakening assertions: the synthetic
Core fixture used an unavailable Ecto UUID helper, and a test helper collided
with `Kernel.binding/0`. Strict Credo prompted decomposition of three predicates.
Next's first build exposed a static availability flag; `connection()` now forces
request-time rendering. Tests and the final build passed after these corrections.

Core and compiled MCP artifacts were reused unchanged. Full Go/Core suites were
not repeated because this change touches neither implementation. Existing
workspace/team evidence establishes their earlier results. This verification
does not prove native host support, replication/failover or production readiness.
