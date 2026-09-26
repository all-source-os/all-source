# Verification record

Host: macOS, Elixir 1.19.1, Erlang/OTP 28 (ERTS 16.1.1). Commands executed with
five-minute outer deadlines (`gtimeout -k 15 300`) unless otherwise noted. Mix
needed loopback socket permission. No external Claude test was executed.

## Results

| Check | Observed result | Local log |
|---|---|---|
| Query Service full default suite | 6 doctests, 1,045 tests, 0 failures, 2 skipped, 96 excluded; 25.2s | `/private/tmp/allsource-live-access-suite-20260926.log` |
| MCP full default suite | 639 tests, 0 failures, 5 excluded; 179.7s | `/private/tmp/allsource-customer-mcp-suite-20260926.log` |
| Final actual Core + HTTP + compiled MCP and eligibility tests | 14 tests, 0 failures, 1 skipped; 3.9s. The skipped test is optional native Claude; 13 executed. | `/private/tmp/allsource-customer-stack-final-20260926.log` |
| Final MCP protocol regressions | 3 tests, 0 failures | `/private/tmp/allsource-customer-mcp-unit-final-20260926.log` |
| Both apps: format check and warnings-as-errors compile | passed | `/private/tmp/allsource-customer-qs-compile-final-20260926.log`, `/private/tmp/allsource-customer-mcp-compile-final-20260926.log` |
| Query Service strict Credo | 239 files, 73 enabled checks, no issues | `/private/tmp/allsource-customer-qs-credo-final-20260926.log` |
| MCP strict Credo | 41 files, 2 enabled repository checks, no issues | `/private/tmp/allsource-customer-mcp-credo-final-20260926.log` |
| Query Service Dialyzer | passed; 7 existing suppressed errors, 1 unnecessary skip, filters unchanged | `/private/tmp/allsource-customer-qs-dialyzer-final-20260926.log` |
| MCP Dialyzer | passed; 0 errors, 0 skipped | `/private/tmp/allsource-customer-mcp-dialyzer-final-20260926.log` |
| Final production MCP release build | version 0.25.1 built locally; not published/deployed | `/private/tmp/allsource-customer-mcp-release-final-20260926.log` |

The full default suites ran before the optional native-host driver, additional
HTTP JWT/text-parity assertions, and final protocol error/log wording changes.
Final compile/lint/type checks, the real stack and the focused MCP protocol tests
ran after those changes. No dependency lockfile or existing Dialyzer filter was
modified. Raw full-suite logs remain local because unrelated test output does
not belong in the evidence package.

## Reproduction

From `apps/mcp-server-elixir`:

```text
MIX_ENV=test gtimeout -k 15 300 mix compile --warnings-as-errors
MIX_ENV=test gtimeout -k 15 300 mix credo --strict
gtimeout -k 15 60 mix format --check-formatted
MIX_ENV=test gtimeout -k 15 300 mix test
MIX_ENV=test gtimeout -k 15 300 mix test test/mcp_server_elixir/protocol/customer_review_test.exs
MIX_ENV=test gtimeout -k 15 300 mix dialyzer
MIX_ENV=prod gtimeout -k 15 300 mix release --overwrite
```

From `apps/query-service`:

```text
env -u ALLSOURCE_CLAUDE_BINARY -u ALLSOURCE_CLAUDE_TRACE MIX_ENV=test gtimeout -k 15 300 mix compile --warnings-as-errors
env -u ALLSOURCE_CLAUDE_BINARY -u ALLSOURCE_CLAUDE_TRACE MIX_ENV=test gtimeout -k 15 300 mix credo --strict
gtimeout -k 15 60 mix format --check-formatted
MIX_ENV=test gtimeout -k 15 300 mix test
env -u ALLSOURCE_CLAUDE_BINARY -u ALLSOURCE_CLAUDE_TRACE MIX_ENV=test gtimeout -k 15 300 mix dialyzer
```

Final local stack, also from `apps/query-service`:

```text
env -u ALLSOURCE_CLAUDE_BINARY -u ALLSOURCE_CLAUDE_TRACE \
  MIX_ENV=test \
  ALLSOURCE_CORE_BINARY=/Users/decebaldobrica/Projects/founder-mode/all-source/target/debug/allsource-core \
  ALLSOURCE_CUSTOMER_MCP_BINARY=/Users/decebaldobrica/Projects/founder-mode/all-source/apps/mcp-server-elixir/_build/prod/rel/mcp_server_elixir/bin/mcp_server_elixir \
  gtimeout -k 15 300 mix test --include integration \
  test/query_service_ex/integration/customer_agent_http_test.exs \
  test/query_service_ex/integration/customer_agent_grant_core_test.exs \
  test/query_service_ex/domain/customer_agent/eligibility_test.exs
```

Core binary was built from unchanged Core sources with
`cargo build --offline -p allsource-core --bin allsource-core --features enterprise`.
Its SHA-256 remains `272e27eb05fc76372587aeec7fd94360dbe8536ef7ddc01fba7e7c93908ce918`,
matching the prior grant-isolation proof. Enterprise features are needed for
the actual tenant routes used by the fixture. This is local single-node WAL
recovery evidence, not a deployment/failover test.

The final MCP release archive hash covers the complete local artifact, not only
the launcher script. See `artifact-sha256.txt`; build outputs themselves are not
committed. Source hashes in `source-sha256.txt` are relative to the repository
root. Check them with `shasum -a 256 -c <manifest>` from that root.

The optional native-host driver is intentionally absent from these reproduction
commands. Running it would send the three customer skill files and synthetic
test context to Anthropic. Automatic approval review rejected that operation
pending explicit authorization. Its presence in the test tree is not native
host evidence, installation approval, private-data consent or a customer outcome.
