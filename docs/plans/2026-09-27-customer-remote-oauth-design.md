# Remote customer connection authorization

Task `t-7ce02d`, continuing the approved customer-agent contract. Implementation
is not a release or proof of a complete native/web host journey.

## Decision

Reuse Query Service's verified human sessions, current membership/entitlement
checks, consent registry and existing Core conditional writes. The hosted client
is initially a pre-registered public `claude-ai` client with exactly
`https://claude.ai/api/mcp/auth_callback`. This requires entering that client ID
in Claude's advanced custom-connector settings. It is not DCR or CIMD support.
Local Claude Code keeps the existing OS-owner-bound stdio path.

The alternatives were dynamic client registration (adds public registration
state and abuse handling) and CIMD (adds metadata-fetch/cache/SSRF policy).
Pre-registration is sufficient for the first bounded connector and matches the
MCP registration rules; automatic registration can be added only with its own
validation. A separate OAuth service/database would duplicate product authority.

## Authorization and redemption

Accept only authorization-code requests, S256 PKCE, exact client/redirect and
configured resource, and the single `allsource.review` scope. That scope maps to
the already-consented context and syntax-validation operations. It gives no
source access, proposal persistence, approval or execution authority. The
consent screen must show the selected host, callback hostname and field list.
Browser session, CSRF/origin protection and fresh consent are required before
calling the authorization service. OAuth consent never approves a proposal.

Issuance first persists the existing grant/consent receipt, then returns an
authenticated-encrypted authorization code with a five-minute lifetime. The code
contains the random credential and exact binding/PKCE request inside encryption;
no identity or reusable access token appears in plaintext URLs. Use the existing
server secret through Plug.Crypto's purpose-separated key derivation. Rotation
invalidates pending codes. The code, verifier and token must never enter logs,
analytics or durable plaintext records.

Remote grants cannot authenticate until redemption writes a separate, conditional
Core activation receipt. Only the first absent-to-present write succeeds. Any
second redemption invalidates that grant through the existing independent
revocation marker, including a replay racing the first request. A crash after
activation but before response requires reconnect; do not return a credential
again. An uncertain write fails closed. Replaying an old registry snapshot cannot
erase the independent activation or revocation receipt. Arbitrary Core admin
deletion remains outside this threat model.

Pending authorizations count toward existing 16-live/64-per-day issuance limits;
they expire with their one-hour grant if never redeemed. Consent receipts keep
the original acceptance time. Remote access tokens will carry an encrypted
binding envelope so the remote MCP transport never accepts tenant/subject
arguments from the model. Every use still rechecks current Core authority.
No refresh token or `offline_access` scope is initially advertised; expiry
requires reconnect. Tombstone/history retention remains an existing open gate.
All Query Service readers must support the activation requirement before any
remote issuance is enabled; older readers do not distinguish pending remote
grants. Expired codes are rejected before activation; replay revocation applies
to a still-valid code presented again with its correct PKCE verifier.

## Delivery and verification

The existing MCP protocol implementation will serve the same restricted tools
over HTTP, with explicit per-request authorization context. Do not copy MCP
protocol source into Query Service or create a second generic tool surface.
Discovery must provide RFC 9728/RFC 8414 metadata and a 401 challenge. Token
exchange must accept bounded form-urlencoded bodies; duplicates and unexpected
parameters fail. The browser consent route uses existing product authentication
and a cookie-only proxy. Secrets remain out of GET query parameters other than
the short-lived, PKCE-bound OAuth code required by the authorization redirect.

Tests must cover exact redirects (including query/fragment/encoding variants),
S256 vectors, client/resource/scope substitution, expiry, changed membership and
billing, generic session/agent separation, concurrent redemption, revocation,
registry restoration and actual Core SIGKILL/restart. Protocol and browser
integration, native Claude execution, deployed latency/consistency and hosted
distribution remain required before enabling the feature. No model call or
production grant is authorized by a local test.

## Sources checked 27 September 2026

- [Claude connector authentication](https://claude.com/docs/connectors/building/authentication):
  hosted callback, public pre-registration, S256, form token requests and discovery.
- [MCP authorization](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization):
  exact resource binding, registration alternatives and bearer handling.
- Installed `plug_crypto` implementation: purpose-specific key derivation,
  authenticated encryption and bounded-age verification; no custom cipher.
