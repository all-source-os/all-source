# Customer remote HTTP authorization evidence

Task: `t-7ce02d`. Base: `6e8121b6830c96aa54d5644e574ec1e0f7e6f581`.
This is local implementation and verification, not production activation or a
completed customer-agent delivery contract. All access-bead acceptance criteria
remain open until the remaining host, rollout and product-authority gates pass.

## Implemented boundary

- The existing Query Service owns discovery, pending-request encryption, human
  authorization and form token exchange. Existing Core grants, conditional
  activation, membership, entitlement and independent revocation remain authority.
- The hosted client is the exact pre-registered public `claude-ai` client with
  the fixed Claude callback, S256 PKCE and `allsource.review` scope. No dynamic
  registration, client-metadata discovery, refresh token or offline scope is advertised.
- A ten-minute, purpose-separated encrypted request moves into an HttpOnly,
  SameSite=Lax cookie. Production cookies are Secure. Consent uses a clean URL,
  same-origin POST, verified product session and explicit checkbox. Cancel
  issues no grant. Sign-in recovery clears a stale session while preserving the
  pending request. Existing OAuth login returns to pending consent.
- Token exchange returns an encrypted bearer envelope, not a product session or
  plaintext owner binding. The server injects the validated binding. Raw local
  grants, human JWTs and authorization codes cannot authenticate the remote tools.
- The existing exclusive MCP profile serves its same two tools over stateless
  JSON Streamable HTTP. Every request checks live authorization; tools recheck
  when called. No general Core/admin tools, resources or final approval methods
  become available. Invalid Origin is denied. Notifications return empty 202;
  malformed JSON/batches return 400; GET without SSE returns 405 after auth.
- Public Next routes strip browser cookies from remote MCP and token exchange.
  Discovery and 401 challenges connect the surfaces. JSON/form body limits,
  upstream deadlines, no redirects, no-store and no-referrer apply. Query Service
  applies admission and tenant limits, including invalid remote credentials before
  decryption. Rate limits use the existing per-process ETS limiter, not a new
  distributed/global quota claim.
- Global Next security headers initially overrode OAuth's referrer policy. The
  production HTTP test caught this; route-specific headers now preserve
  `no-referrer`. Only the consent page permits the exact Claude callback in its
  form-action policy, needed for the final form POST redirect.

## Verification completed

| Check | Result |
| --- | --- |
| Full Query Service suite | 6 doctests, 1,057 tests, 0 failures, 2 skipped, 112 excluded |
| Full MCP suite | 646 tests, 0 failures, 5 excluded |
| Focused real Core + separately compiled MCP stdio/HTTP regression | 22 tests, 0 failures, 3 intentional skips; 19 executed |
| Web connection/proxy/consent tests | 32 tests passed across 5 files |
| Web TypeScript and production build | Passed |
| Elixir compile, format, configured Credo and Dialyzer | Passed; existing QS 7 filtered warnings / 1 unnecessary filter unchanged; MCP 0 errors |

The focused run covers malformed/duplicate form fields, wrong client/resource/
verifier, encrypted-envelope purpose and expiry, raw credential rejection,
consent/session/tenant restrictions, changed billing/membership, revoked access,
replay races and Core crash recovery from the prior internal PKCE tests. The
compiled MCP HTTP test proves discovery, initialization, notifications, exact
tool list, context, unresolved validation, denied admin/resource operations,
Origin/query/size rejection, code replay revocation and reconnect. A separate
real-socket MCP test loses authority between preflight and tool execution and
still returns a valid 401 response after reading the request body.

The opt-in `CustomerRemoteWeb.verify/0` test ran against a production Next build
on port 4344, actual Query Service handlers on 4345, compiled MCP release on 4346
and an isolated Core database. It proved public discovery, unauthenticated MCP
challenge, clean authorization redirect, HttpOnly cookie, anonymous and signed-in
consent HTML, local identity exchange, CSRF/missing-consent rejection, fixed
authorization-code redirect, public token exchange, tool discovery and replay
revocation through the web proxy. It inspected the Claude redirect without
following it. No credentials or synthetic test data were sent to Anthropic.
That fixture run: 6 tests, 0 failures, 1 unrelated manual-browser skip.

Browser evidence is narrower: Codex's in-app browser displayed the clean consent
page, readable field list, callback hostname, sign-in and cancel controls. Its
form navigation did not complete. A sanitized CDP observation reported
`Network.loadingFailed`, `blockedReason: inspector`,
`net::ERR_BLOCKED_BY_CLIENT` for the decision POST. No application error appeared
in the browser log. A long tool delay also let the first bounded fixture expire;
a fresh fixture reproduced the blocked form. No bypass was attempted. Therefore
interactive sign-in/consent and actual hosted Claude are **not verified** by the
HTTP test or page screenshot. Fixtures were stopped and their private temporary
Core directories removed by test cleanup.

## Remaining gates

Keep all new rollout flags off. Native/hosted Claude verification, existing
external-processing approval, deploy topology/latency, complete per-replica
rollout, legacy owner migration, trial MCP entitlement policy and retention are
still open. Source authority, stored proposals, product review display and final
human approval remain unavailable; these tools only check eligibility and syntax.
The existing legacy Google/GitHub auth callback still receives a product JWT in
its query string; this change does not close that earlier session-handoff gate.

The MCP-to-Query HTTP adapter caps accepted response bytes after its trusted
upstream response is buffered; it is not a streaming memory-bound proof. Browser
consent has local UI/HTTP evidence only. The production endpoint has not been
enabled or exercised. No external model call, charge or Google OAuth publication
was made. Google JobBreakdown remains External/Testing with its single test user.

## Reproduction

Build the enterprise Core and existing MCP release independently. From
`apps/query-service`, set `ALLSOURCE_CORE_BINARY` and
`ALLSOURCE_CUSTOMER_MCP_BINARY` to those absolute binary paths. Unset
`ALLSOURCE_CLAUDE_BINARY` and `ALLSOURCE_CLAUDE_TRACE`. Run:

```text
mix test --include integration \
  test/query_service_ex/integration/customer_remote_http_test.exs \
  test/query_service_ex/integration/customer_remote_mcp_test.exs \
  test/query_service_ex/integration/customer_remote_authorization_test.exs \
  test/query_service_ex/integration/customer_connections_test.exs \
  test/query_service_ex/integration/customer_agent_http_test.exs \
  test/query_service_ex/infrastructure/adapters/customer_remote_tokens_test.exs
```

The optional full web fixture requires a production Next preview on
`http://127.0.0.1:4344`, Query Service and Control Plane URLs both pointed to the
synthetic fixture at 4345, MCP upstream at 4346, issuer `https://www.example.test`,
and both web connection flags true. Add `ALLSOURCE_REMOTE_BROWSER_FIXTURE=1`
and `ALLSOURCE_REMOTE_WEB_URL=http://127.0.0.1:4344`, then include
`--include browser_fixture` with the remote MCP and connections test files.
The fixture has a 240-second deadline and local `/fixture/stop` route. These
synthetic test settings must never be used as production identity services.

Source and tested artifact SHA-256 manifests accompany this evidence. The prior
base commit's CI, security and container build runs all completed successfully;
that does not substitute for CI on this change.
The shared worktree contained an unrelated edited `product-analytics.ts` during
the local web build. It is not included in this change; its hash is recorded as
artifact context. A clean committed-tree build remains required for deployment.

References: [design](../../plans/2026-09-27-customer-remote-oauth-design.md),
[rollout configuration](../../runbooks/CUSTOMER_REMOTE_CONNECTIONS.md),
[MCP HTTP transport](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports),
[Claude authentication](https://claude.com/docs/connectors/building/authentication).
# CI formatter follow-up

Query Service CI run `36310262921` failed its Elixir 1.18 formatter on two
constructs accepted by local Elixir 1.19. The follow-up uses a shorter service
alias and separates HTTP tuple matching from payload assertions, preserving
the same assertions. Local format, strict Credo and actual Core/compiled MCP
HTTP regression passed: 6 tests, 0 failures, 1 optional browser fixture skipped.
The original manifests below describe the original implementation, not this
formatter-only source delta. MCP CI for `d1ba62e0` passed; deployment and host
verification remain gated as described below.
