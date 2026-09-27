# Reproduction

Run from `apps/mcp-server-elixir` with installed dependencies:

```console
MIX_ENV=test mix compile --warnings-as-errors
MIX_ENV=test mix credo --strict
MIX_ENV=test mix dialyzer
MIX_ENV=test timeout -k 5 360 mix test --trace
MIX_ENV=prod timeout -k 5 120 mix release --overwrite
```

Run from `apps/query-service`:

```console
MIX_ENV=test mix compile --warnings-as-errors
MIX_ENV=test mix credo --strict
MIX_ENV=test mix dialyzer
MIX_ENV=test timeout -k 5 180 mix test
ALLSOURCE_CORE_BINARY=/private/tmp/allsource-query-admission-core-JlQWNk/allsource-core \
ALLSOURCE_CUSTOMER_MCP_BINARY=/Users/decebaldobrica/Projects/founder-mode/all-source/apps/mcp-server-elixir/_build/prod/rel/mcp_server_elixir/bin/mcp_server_elixir \
MIX_ENV=test timeout -k 5 150 mix test \
  test/query_service_ex/integration/customer_evidence_transport_test.exs \
  test/query_service_ex/integration/customer_agent_http_test.exs \
  test/query_service_ex/integration/customer_remote_mcp_test.exs \
  --include integration --seed 42017
```

All new/modified Elixir files were formatted explicitly and checked with
`mix format --check-formatted`. Broad repository formatting was not performed.
The immutable Core binary was built from the canonical admission candidate;
its SHA-256 and the packaged MCP archive SHA-256 are in `artifacts.sha256`.
The synthetic Core and MCP processes use private temporary directories,
loopback listeners and bounded cleanup. No production credentials are required.

Per-app code remains isolated. Tests reach the independently compiled MCP release
through stdio/HTTP, not by importing another application's modules. The broader
host-evaluation XML is a planned native-model evaluation, not a test execution.

Prior pushed source `14ee1ba8`: CI, Docker Build and Container CI passed;
Security Scanning was still running when checked during this increment. New
commit CI status must be checked separately; local checks are not remote CI.
