# Verification record

Commands run from the indicated application directory with bounded process
deadlines. Tests use synthetic identities; test logs are local and uncommitted.

Control Plane:

```text
ALLSOURCE_CORE_BINARY=/absolute/path/to/target/debug/allsource-core go test -race ./...
golangci-lint run
```

Query Service (existing compiled MCP release, no external model):

```text
env -u ALLSOURCE_CLAUDE_BINARY -u ALLSOURCE_CLAUDE_TRACE \
  MIX_ENV=test \
  ALLSOURCE_CORE_BINARY=/absolute/path/to/target/debug/allsource-core \
  ALLSOURCE_CUSTOMER_MCP_BINARY=/absolute/path/to/apps/mcp-server-elixir/_build/prod/rel/mcp_server_elixir/bin/mcp_server_elixir \
  mix test --include integration \
  test/query_service_ex/integration/customer_agent_http_test.exs \
  test/query_service_ex/integration/customer_agent_grant_core_test.exs \
  test/query_service_ex/domain/customer_agent/eligibility_test.exs
mix credo --strict
mix dialyzer
```

Web:

```text
bun install --frozen-lockfile
bun run type-check
bun run test
bun run test src/__tests__/team-proxy.test.ts src/__tests__/team-membership-ui.test.tsx
bun run build
```

Browser fixture, launched from Control Plane:

```text
ALLSOURCE_TEAM_BROWSER_FIXTURE=1 ALLSOURCE_CORE_BINARY=/absolute/path/to/target/debug/allsource-core \
  go test -run '^TestTeamBrowserFixture$' -count=1 -v .
```

Set `CONTROL_PLANE_INTERNAL_URL` and `QUERY_SERVICE_URL` to the printed loopback
fixture URL, `NEXT_PUBLIC_APP_URL` to the local web origin and
`NEXT_PUBLIC_POSTHOG_KEY` to empty. Start web dev on that origin. The fixture
accepts only `owner@example.test` and `member@example.test` at the synthetic
login endpoint. This is not a real password/provider test. Use the normal web
login form and return to `/dashboard/team`. `POST /fixture/stop` stops the
fixture and its Core; it otherwise ends after eight minutes.

Local log paths under `/private/tmp/`:

- `allsource-team-full-go-final-20260927.log`: final full race suite.
- `allsource-team-lint-final-20260927.log`: final Go lint.
- `allsource-team-web-full-tests-20260927.log`: 182-test suite before final copy/input-size edits.
- `allsource-team-web-final-focused-20260927.log`: final 17 team tests.
- `allsource-team-web-types-20260927.log`: passing web type check.
- `allsource-team-web-build-20260927.log`: production build.
- `allsource-team-web-final-lint-20260927.log`: targeted Biome result.
- `allsource-team-qs-integration-20260927.log`: real Core/HTTP/MCP regression.
- `allsource-team-qs-credo-20260927.log`, `allsource-team-qs-dialyzer-20260927.log`.
- `allsource-team-browser-fixture-20260927.log`, `allsource-team-web-browser-20260927.log`: browser fixture and web routes.

Earlier verification caught removed imports still needed by agent-key helpers,
obsolete legacy admission mocks, unchecked test errors, test typing issues and
formatting/lint errors. Final review also caught a malformed null cookie payload
that could throw before the proxy's rejection path; it now returns 401 before
any upstream call. These were corrected; no assertions or quality rules
were weakened to make the final checks pass. The browser fixture deliberately
does not serve unrelated notices/logout endpoints; those 404s do not test those
features. No production sign-in blocker was verified from the current browser
state (the original sign-in tab was absent).
