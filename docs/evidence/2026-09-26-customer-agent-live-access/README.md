# Customer review: live access and restricted MCP proof

26 September 2026. Progress on `t-7ce02d` and its dependent bindings work
`t-7b4430`; neither task is complete. Starting commit:
`f9ef017d9370e84d2cdaaf6f9eae3ef1a4fdb9ce`. Source and artifact hashes are stored
beside this document. No production data, grant issuance, deployment or new pilot.

## What the stack now does

The existing Elixir MCP stdio server can run an exclusive customer review
profile. Its only tools are `allsource_review_context` and
`allsource_validate_review_proposal`. They call the existing Query Service over
HTTP, which reuses Core for current grant/revocation, Control Plane team records
and tenant billing/quotas. There is no new database or authentication service.

The profile disables generic Core/admin tools and backend children regardless
of the system-admin flag. It provides strict input schemas, output schemas,
read-only annotations, structuredContent and matching JSON text. The advertised
protocol is MCP `2025-06-18`; JSON parse failures and invalid JSON-RPC requests
have distinct protocol errors. Resources, prompts and arbitrary methods cannot
fall through to the general server.

Current membership is one exact `admin`/`member` record for the opaque subject.
Active non-demo tenant state, persisted MCP scope, subscription status, trial or
subscription deadline and available query budget are required. Unknown data
denies access. Paid conversions retain access despite historical trial dates;
current dunning grace is preserved. No price/tier-to-scope fallback is added.

HTTP context returns `eligibility_verified`, unresolved source authority and
unavailable preparation. Validation uses the actual typed proposal/curated
projection rules and returns `valid_unresolved`, fingerprint, unknowns,
`persisted: false`, `approved: false`. Nothing retrieves events, creates a review
or records a human decision.

## Evidence

The local integration harness starts the actual Rust Core enterprise binary
with authentication enabled, private temporary WAL directories and isolated
synthetic data. It runs the actual Query Service endpoint through Bandit and
starts the compiled MCP release as an owned stdio process. These are real
HTTP/process boundaries, not a mocked successful connector.

Verified cases:

- Actual Control Plane OAuth subject shape, path-safe tenant/client validation,
  exact membership and role checks; absent, duplicate or unknown membership
  denies access.
- Current billing status/scope/quota, exact trial expiry, paid conversion and
  dunning grace; changes after reconnect are read from Core.
- Grant scope, tenant, subject, audience, expiry and revocation; independent
  callers and a new MCP process do not revive revoked access.
- Compiled MCP discovery exposes exactly two tools. Generic query, replay,
  tenant administration and approval requests are rejected even with the
  system-admin environment flag set.
- Both successful tool responses have JSON text identical to structuredContent.
  Proposal validation explicitly reports unresolved source authority and no
  persistence/approval.
- HTTP rejects generic administrator JWTs and forged proposal fields. A real
  70 KiB request returns HTTP 413. Disabled route configuration denies access.
- Request log capture excludes synthetic private markers, grant and subject;
  customer-route query strings are rejected and redacted.
- Existing grant/revocation SIGKILL and WAL recovery scenario still passes,
  including denial after stale tenant metadata and grant-record rewrites.

Initial body-limit implementation failed the real HTTP test: raising inside
the custom body reader closed the client connection instead of delivering 413.
The final route-specific plug responds using the updated connection and halts
before generic parsing. The actual HTTP regression now passes.

Gate results and exact commands are recorded in [verification](verification.md).
Existing default suites exclude integration; the real Core/MCP proof runs
explicitly and must not be inferred from the default suite's green result.

## Configuration boundary

Both new profiles default off. This documents implementation configuration, not
a supported customer installation or permission to expose production grants.

Query Service requires `CUSTOMER_REVIEW_ENABLED=true` and an exact
`CUSTOMER_REVIEW_RESOURCE`. It accepts only POST JSON objects at
`/api/customer-agent/context` and `/api/customer-agent/validate`, with a separate
opaque Bearer grant and an exact binding object. Context accepts only `binding`;
validation accepts only `binding` and `proposal`. Generic JWT/dev/session tenant
fallbacks are not used.

The existing MCP release requires `ALLSOURCE_CUSTOMER_REVIEW=true` and
`CUSTOMER_REVIEW_URL`, `CUSTOMER_REVIEW_GRANT`, `CUSTOMER_REVIEW_TENANT`,
`CUSTOMER_REVIEW_SUBJECT`, `CUSTOMER_REVIEW_CLIENT`, `CUSTOMER_REVIEW_RESOURCE`.
These are process configuration, never model tool arguments. The HTTP origin
must be HTTPS, except literal loopback HTTP used by the local fixture; userinfo,
query, fragment and non-root path are rejected. The client has no redirect or
retry middleware and uses a 65-second request timeout.

HTTP input is bounded at 64 KiB and a five-second body-read timeout before the
generic parser. Errors are fixed and responses use `Cache-Control: no-store`.
The existing per-process limiter gates global admission and authenticated
tenants. Stdio has a 64 KiB post-read validation limit; the HTTP client rejects
responses above 8 KiB after receipt. These checks do not establish a streaming
allocation cap, distributed concurrency budget or production cost guarantee.

## Remaining release gates

- Authenticated issue/revoke UI and authority, durable explicit host/field
  consent, grant-count/retention controls and real local process/owner binding
  or remote OAuth PKCE with exact redirects.
- Normal-account owner provisioning: CP OAuth registration does not currently
  persist the owner in the team list used here. JWT role and tenant slug are not
  safe substitutes. Current agent-trial metadata also lacks hosted MCP scope;
  this change does not silently grant it.
- Source ownership and field resolution, durable idempotent preparation,
  review/status/result storage, product display and separate version-bound
  human gate/receipt.
- Replication/failover and deployed consistency. Repeated current leader reads
  are not a transaction covering membership, billing, revocation and disclosure.
- Native Claude Code and claude.ai journeys, MCP App rendering/accessibility,
  customer distribution/discovery and existing product qualification gates.

An optional Claude Code driver copies all three customer-skill files into a
private test project and configures only the restricted MCP release. It has a
150-second deadline, $2 budget cap and no shell/write/browser tools. Execution
was rejected by automatic approval review pending permission for external
processing of the skill and synthetic context by Anthropic. The rejected
attempt did not run; no host proof or model charge is claimed. The optional
test is skipped in local runs without `ALLSOURCE_CLAUDE_BINARY`.

The complete end-to-end feature remains open. Local connector success and
synthetic eligibility checks are not customer outcome evidence.

Protocol references: [MCP tools](https://modelcontextprotocol.io/specification/2025-06-18/server/tools)
and [stdio transport](https://modelcontextprotocol.io/specification/2025-06-18/basic/transports).
