# Client keys: register, rotate, revoke, and the legacy sunset

A tenant client (first: Longhand's control plane) authenticates to the gateway
with a P-256 keypair. The public key is registered on the tenant; each request
carries a 60-second ES256 assertion the client signs itself. Background and
rejected alternatives:
[the decision record](../decisions/2026-11-07-tenant-clients-registered-es256-keys.md).

Every command here runs against `allsource-control-plane` with an **operator**
session (admin role, human sign-in). API keys, client assertions and legacy
bearers are refused on these routes.

## What the gateway checks

| Claim / header | Rule |
|---|---|
| header `alg`, `kid`, `typ` | `ES256`, the registered kid, `typ` absent or `JWT` |
| `iss` | the registered `client_id` |
| `sub` | the tenant the key is registered on; a `tenant_id` claim must equal it |
| `aud` | one string equal to `ALLSOURCE_ASSERTION_AUDIENCE` (default `https://api.all-source.xyz`); an array is refused |
| `iat`, `exp`, `nbf` | `exp` required, `exp - iat` at most 60 s, 30 s skew either way |
| `jti` | required; logged in the audit log as `assertion_jti`, not checked for reuse |
| role, scopes | from the registration only; claims are ignored |

A tenant's registrations are read from Core at most once per 30 s per gateway
machine; one read answers every kid of that tenant, and concurrent requests share
it. A `kid` that is not a 43-character base64url thumbprint is refused before
Core is read. Reads for tenants not in the cache are capped at 20 per second per
machine (burst 40); a request over the cap is treated as a Core outage. While
Core cannot answer, a key read successfully in the last 5 minutes keeps working;
after that the gateway answers `503` with `Retry-After: 5`. A revoked key is
refused as soon as a gateway machine reads the revocation, and no later read
brings it back.

## Generate a keypair (on the client's machine)

```bash
openssl ecparam -name prime256v1 -genkey -noout | openssl pkcs8 -topk8 -nocrypt -out client-key.pem
```

Keep `client-key.pem` in the client's secret store. Only the public JWK
(`kty`, `crv`, `x`, `y`) is sent to the gateway. Any JWK containing `d` is
refused.

## Register a key

```bash
curl -sS -X POST "https://api.all-source.xyz/api/v1/admin/tenants/$TENANT/client-keys" \
  -H "Authorization: Bearer $OPERATOR_SESSION" -H 'Content-Type: application/json' \
  -d '{"client_id":"longhand-control-plane","jwk":{"kty":"EC","crv":"P-256","x":"...","y":"..."},"scopes":["events:read","events:write","queries:execute","projections:read"]}'
```

The answer is `201` with the `kid` (the RFC 7638 thumbprint the server computed).
Give the client that kid. Repeating the same registration answers `200` with
`already_registered: true` and writes nothing; the same key with a different
client id or scopes answers `409`. `scopes` must be a non-empty subset of
`events:read events:write queries:execute projections:read projections:write schemas:read schemas:write`.
The role is always `serviceaccount`.

List what a tenant has (public keys only):

```bash
curl -sS "https://api.all-source.xyz/api/v1/admin/tenants/$TENANT/client-keys" \
  -H "Authorization: Bearer $OPERATOR_SESSION"
```

## Configure the client

Rust clients use the SDK's `Es256AssertionProvider` (feature `es256-assertion`).
`allsource-mcp` reads `ALLSOURCE_CLIENT_SIGNING_KEY` (PEM or a path),
`ALLSOURCE_CLIENT_ID`, `ALLSOURCE_TENANT_ID`, `ALLSOURCE_CLIENT_KID` and
`ALLSOURCE_ASSERTION_AUDIENCE`. A client should fail its own boot when the key
is missing, rather than start without a credential.

## Rotate kid1 to kid2

1. Generate the new keypair and **register kid2** as above.
2. **Wait 30 s.** That is the gateway's cache TTL; a machine that read the tenant
   just before the registration may answer `asserted_unregistered` for kid2
   until then.
3. **Switch the client's secret everywhere** it runs to the new private key and
   kid2. Deploy or restart each instance.
4. **Wait 60 s + 30 s.** An assertion signed with kid1 just before the switch is
   valid until its `exp` (60 s) plus the 30 s skew.
5. Confirm no kid1 traffic remains (denials and audit log below), then
   **revoke kid1**:

```bash
curl -sS -X DELETE "https://api.all-source.xyz/api/v1/admin/tenants/$TENANT/client-keys/$KID1" \
  -H "Authorization: Bearer $OPERATOR_SESSION"
```

A revocation is permanent: re-registering the same key answers `409 kid_revoked`.
On a leaked key, skip the waits and revoke first; the client is down until it has
a new registered key.

## Reading refusals

Every refusal answers `{"error", "reason", "message"}` and is counted in
`control_plane_auth_denials_total{mode="enforce", reason, tenant}` on the private
metrics listener. `tenant` is `unverified` until the signature has been checked.

| reason | Meaning | Usual fix |
|---|---|---|
| `asserted_signature` | malformed token or `kid`, missing `jti`, or signature does not verify with the registered key | client is signing with a different key than the one registered |
| `asserted_audience` | `aud` is not the single configured string | set the client's audience to `ALLSOURCE_ASSERTION_AUDIENCE` |
| `asserted_skew` | `iat`/`nbf` more than 30 s ahead, or `exp - iat` over 60 s | fix the client clock or its token lifetime |
| `asserted_expired` | `exp` missing or more than 30 s past | client reused an old assertion |
| `asserted_unregistered` | no registration for (tenant, kid), or `iss`/tenant mismatch | register the key, or fix `client_id`/`sub`; wait 30 s after registering |
| `asserted_revoked` | the kid was revoked | rotate to a new key |
| `asserted_unavailable` | Core unreadable and no good read in the last 5 min (503) | Core outage; the client should retry after 5 s |
| `history_invalid`, `incomplete_authority` | the `clientkey:<tenant>` stream failed the strict fold | investigate the stream; nothing outside the operator routes can write it |
| `scope` | the registration's scopes do not cover the route (403) | register a key with the needed scopes |

## Legacy key sunset

`ALLSOURCE_LEGACY_KEY_SUNSET` is an RFC 3339 instant set on **both**
`allsource-control-plane` and `allsource-query`. It governs credentials the key
authority classes as unregistered: HS256 keys with no `apikey.created` record,
such as keys minted by the old onboarding flow (`SignAPIKey`).

- **Unset**: today's behaviour. `shadow` serves them, `enforce` refuses them.
- **Set, before the instant**: they are served in `shadow` and `enforce`, and
  every such request is counted. The gateway logs the configured value at boot
  (`Legacy API key sunset: ...`).
- **Set, at or after the instant**: they are refused with
  `401 {"error":"unauthorized","reason":"legacy_key_sunset","message":"This API key predates registered keys and stopped working on <date>. Register a client key or create a dashboard key."}`
  (the Query Service nests the same `reason` and `message` under `error`).
- Revoked keys are refused at all times. That includes a legacy key whose id an
  `apikey.revoked` event names although it has no `apikey.created` record: the
  sunset never serves it, so `enforce` refuses it, while `shadow` keeps serving
  it as it did without a sunset. `off` mode skips the authority and the sunset
  with it.
- Query Service telemetry: a managed-key token with no record now reports
  `reason: :unregistered` on `[:query_service_ex, :auth, :key_authority_denial]`
  where it reported `:invalid_key`. A dashboard or alert filtering on
  `invalid_key` for never-registered keys must add `unregistered`. A key a stray
  revocation names still reports `:invalid_key`.

Procedure:

1. On the deploy that ships client keys, set the sunset to the deploy date plus
   30 days on both apps, and set `ALLSOURCE_KEY_AUTHORITY_MODE=enforce`. With the
   sunset set, `enforce` no longer refuses unregistered keys before the instant,
   so the flip is safe for legacy holders.

   ```bash
   fly secrets set --stage -a allsource-control-plane ALLSOURCE_LEGACY_KEY_SUNSET=2026-12-07T00:00:00Z ALLSOURCE_KEY_AUTHORITY_MODE=enforce
   fly secrets set --stage -a allsource-query ALLSOURCE_LEGACY_KEY_SUNSET=2026-12-07T00:00:00Z ALLSOURCE_KEY_AUTHORITY_MODE=enforce
   ```

2. Watch `control_plane_legacy_key_requests_total{tenant}` on the gateway (and
   the `[:query_service_ex, :auth, :legacy_key_served]` telemetry event on the
   Query Service). Each tenant still using a legacy key also appears in the logs
   at most once a minute as `legacy API key served: tenant=...`. Move each of
   those tenants to a client key or a dashboard key before the instant.
3. At the instant those keys get the 401 above. A customer who reports it needs a
   registered client key (this runbook) or a dashboard key.
4. On sunset day, retire the legacy canary from `CONSUMER_CANARY_CREDENTIALS` as
   rule 3 of the [consumer access gate](CONSUMER_ACCESS_GATE.md) requires,
   recording the migrated tenants in that change.
