# Tenant clients authenticate with registered ES256 keys — 7 November 2026

Status: accepted. First client: Longhand's control plane, tenant
`onboard-control-plane-at-technical-leaders-com`.

## Context

Every tenant client today authenticates to the gateway with a long-lived
HS256 bearer signed with the platform's shared `JWT_SECRET`. Three failures in
two weeks trace back to that shape:

- **2026-10-26, legacy keys locked out (#363).** The managed-key authority
  started refusing keys with no `apikey.created` record. Keys minted by the old
  onboarding flow (`SignAPIKey`) have none, so Longhand's key was refused for
  2 h 29 m and again for 8 m
  ([report](../reports/2026-10-26-client-access-outage.html)). The fix was to
  serve unregistered keys again, which leaves them valid indefinitely.
- **2026-11-06, no-expiry 401 on the `7e08966d` gateway.** A key with no
  expiry was answered 401 by the gateway running build `7e08966d`. A client
  holding a static bearer cannot replace it on its own; recovery needed an
  operator.
- **The shared HS256 secret.** Any service holding `JWT_SECRET` can mint a
  bearer for any tenant and role. A client's key is a copy of something the
  platform can forge, and a leaked key is valid until someone notices.

## Decision

A tenant client holds a P-256 keypair. An operator registers the public key on
the tenant; the private key never leaves the client. Each request carries a
short-lived ES256 assertion the client signs itself. Legacy unregistered keys
keep working for 30 days after the deploy, then stop.

1. **Registrations live on `clientkey:<tenant>`**, a new entity, with event
   types `clientkey.registered` and `clientkey.revoked`. A registration carries
   `kid, client_id, tenant_id, alg: "ES256", jwk {kty: EC, crv: P-256, x, y},
   role: "serviceaccount", scopes, active, created_at, created_by`; a revocation
   carries `kid, revoked_at, revoked_by`. `kid` is the RFC 7638 thumbprint, so a
   kid cannot be re-registered with a different key. The fold is as strict as
   the managed-key fold: retained-entity-v1 integrity, contiguous versions,
   monotone timestamps, and an unknown event type invalidates the history.
2. **Public ingest refuses the new stream** on both the gateway and the Query
   Service (`clientkey:` entities, `clientkey.` event types), shipped before any
   writer exists.
3. **The gateway verifies assertions** before the HS256 path. A bearer whose
   header says `alg: ES256` with a `kid` (and `typ` absent or `JWT`) goes to the
   asserted path and never falls back. Claims: `iss` = registered `client_id`,
   `sub` = tenant (and any `tenant_id` claim equals it), `aud` = the single string
   `ALLSOURCE_ASSERTION_AUDIENCE` (default `https://api.all-source.xyz`), `exp`
   required, `exp - iat <= 60s`, ±30 s skew, `jti` required and logged for audit.
   Role and scopes come only from the registration. The asserted path enforces
   whatever `ALLSOURCE_KEY_AUTHORITY_MODE` says.
4. **Lookup and cache**: one retained-entity read of `clientkey:<tenant>` per
   tenant per 30 s per gateway machine, shared by every kid of the tenant and by
   concurrent requests, which bounds how long a revocation takes to apply. The
   read happens before the signature is checked, so the `kid` must have the
   shape of a thumbprint and cache misses are capped at 20 reads per second per
   machine. While Core cannot answer, a key read successfully in the last
   5 minutes is honoured, a revoked one never; after that the gateway answers
   503 with `Retry-After: 5`. A revocation once observed is never undone by a
   later or slower read.
5. **Operator routes** under the admin group: `POST`, `GET` and
   `DELETE /api/v1/admin/tenants/:id/client-keys[/:kid]`. The server computes
   the kid, refuses a JWK carrying `d` or a point off the curve, and writes to
   Core with a control-plane delegation token. No tenant credential can reach
   these routes.
6. **Legacy sunset**: `ALLSOURCE_LEGACY_KEY_SUNSET` (RFC 3339) on the gateway
   and the Query Service. Until that instant an unregistered key is served even
   under `enforce`, and counted per tenant
   (`control_plane_legacy_key_requests_total`); after it, the key gets
   `401 {"reason":"legacy_key_sunset"}`. Revoked keys are refused at all times.
   Unset keeps today's behaviour.
7. **Canary**: `CONSUMER_CANARY_CREDENTIALS` accepts an asserted entry that the
   gateway signs fresh on every probe round. The Query Service reports such an
   entry as skipped until it verifies assertions itself.
8. **One client implementation**: the Rust SDK gains `CredentialProvider` and,
   behind the `es256-assertion` feature, `Es256AssertionProvider`. Longhand,
   `better-auth-allsource` and `allsource-mcp` use it rather than each minting
   tokens.

   Parity note: the TypeScript, Python and Go SDKs and the Elixir MCP server
   (`apps/mcp-server-elixir`) now carry their own ES256 signers, so non-Rust
   clients do not need the Rust SDK. Each is checked
   against the shared fixtures in `sdks/rust/tests/fixtures/`
   (`es256_interop_test_key.pem`, `es256_interop_token.jwt`), which the gateway
   also verifies in `asserted_keys_interop_test.go`. Same claims, same 60 s
   lifetime, same rule of sending only `Authorization`.

## Rejected alternatives

- **An AllSource-published JWKS alone.** Rotating platform-signed tokens through
  a JWKS still leaves the platform holding every client's signing authority.
  Nothing the client holds is permanent; each token is still something the
  platform issued and can lose.
- **A remote client JWKS URL.** The gateway would fetch each client's keys from
  the client. Authentication would then depend on the client's own endpoint
  being up and honest, and a client outage would look like an auth failure.
- **Reusing `apikey:<tenant>`.** The managed-key folds on the gateway
  (`managed_keys.go`, default branch) and the Query Service
  (`api_key_authority.ex`, catch-all) treat an unknown event type as an invalid
  history. A `clientkey.*` event there would lock out every key of that tenant.
- **Single-use `jti`.** Replay protection inside the 60 s window needs a shared
  store of seen ids across gateway machines. Deferred: `jti` is logged, and the
  window is short.

## Consequences

- The sunset is 30 days after the deploy. Set
  `ALLSOURCE_LEGACY_KEY_SUNSET` to the deploy date plus 30 days on both apps; watch
  `control_plane_legacy_key_requests_total` fall to zero before that instant.
  Procedure: [`CLIENT_KEY_ROTATION.md`](../runbooks/CLIENT_KEY_ROTATION.md).
- The Query Service does not verify assertions yet. Asserted clients go through
  the gateway; the QS consumer gate skips the asserted canary.
- Revocation takes up to 30 s to apply on every gateway machine (the cache TTL),
  and an assertion already issued stays usable until its `exp` plus 30 s skew.
- Four ways Longhand can fail silently after switching, each with its guard:
  1. **The signing key is missing or unreadable at boot.** The SDK provider
     refuses the key at construction; Longhand must fail its own boot rather
     than fall back to no credential.
  2. **The kid does not match the key.** The gateway answers
     `401 asserted_unregistered`; the provider does not check this, so the first
     request after deploy is the test.
  3. **Clock drift beyond 30 s.** Every request fails with `asserted_skew` or
     `asserted_expired`; the denials metric shows the reason.
  4. **The legacy key is still configured somewhere.** It keeps working until
     the sunset and then stops; `control_plane_legacy_key_requests_total{tenant}`
     shows it before that happens.
