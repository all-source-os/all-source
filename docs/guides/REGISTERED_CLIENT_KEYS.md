# Registered client keys

A registered client key lets a service authenticate to the hosted gateway
(`https://api.all-source.xyz`) without holding a long-lived bearer. The service
keeps a P-256 private key. A platform operator registers the public half on the
tenant. On every request the service signs a 60-second ES256 assertion and sends
it as `Authorization: Bearer <assertion>`.

Use one when a long-running service, agent or MCP server talks to the hosted
gateway. Dashboard API keys (`ask_...`) still work and are the simpler choice for
scripts and local tools.

Background and rejected alternatives:
[the decision record](../decisions/2026-11-07-tenant-clients-registered-es256-keys.md).
Operator procedure (register, rotate, revoke, sunset):
[CLIENT_KEY_ROTATION.md](../runbooks/CLIENT_KEY_ROTATION.md).

## The three credential kinds

| Credential | Lifetime | Issued by | Typical holder |
|---|---|---|---|
| Dashboard API key (`ask_...`) | until revoked | Dashboard, API Keys page | scripts, CLIs, `chronis` |
| Registered client key | the keypair lives until revoked; each assertion lives 60 s | the client signs; an operator registers the public key | services, agents, `allsource-mcp --remote` |
| Managed onboarding key (HS256 JWT from the old onboarding flow, no registration record) | until `ALLSOURCE_LEGACY_KEY_SUNSET` | the old onboarding `SignAPIKey` | early tenants |

After `ALLSOURCE_LEGACY_KEY_SUNSET`, only the third kind stops working: such a
key gets `401` with reason `legacy_key_sunset`. Dashboard keys are not affected.

## 1. Generate a keypair

On the machine that will hold the key:

```bash
openssl ecparam -name prime256v1 -genkey -noout \
  | openssl pkcs8 -topk8 -nocrypt -out client-key.pem
```

Keep `client-key.pem` in your secret store. It never leaves the client.

## 2. Build the public JWK and compute the kid

The `kid` is the RFC 7638 SHA-256 thumbprint of the public JWK, base64url
without padding. The gateway computes it at registration and returns it; the
local value is a cross-check.

```bash
b64url() { openssl base64 -A | tr '+/' '-_' | tr -d '='; }

# The last 64 bytes of a P-256 SubjectPublicKeyInfo are x || y.
openssl ec -in client-key.pem -pubout -outform DER 2>/dev/null | tail -c 64 > pub.raw
X=$(head -c 32 pub.raw | b64url)
Y=$(tail -c 32 pub.raw | b64url)

printf '{"kty":"EC","crv":"P-256","x":"%s","y":"%s"}\n' "$X" "$Y" > client-key.jwk.json

# RFC 7638: members in lexicographic order, no whitespace.
printf '{"crv":"P-256","kty":"EC","x":"%s","y":"%s"}' "$X" "$Y" \
  | openssl dgst -sha256 -binary | b64url; echo
```

The kid is 43 characters. Send only `client-key.jwk.json` (`kty`, `crv`, `x`,
`y`). A JWK that contains `d` is refused.

## 3. Ask an operator to register it

Registration is operator-only. Tenant credentials, API keys and assertions are
refused on these routes, so send the operator:

- the tenant id,
- a `client_id` (1-200 characters; it becomes the assertion's `iss`),
- the public JWK,
- the scopes the service needs, a non-empty subset of `events:read`,
  `events:write`, `queries:execute`, `projections:read`, `projections:write`,
  `schemas:read`, `schemas:write`.

The operator calls `POST /api/v1/admin/tenants/:id/client-keys` with
`{"client_id", "jwk", "scopes"}` and gives you back the `kid` from the `201`
answer. Compare it with the one you computed. The role is always
`serviceaccount`; role and scopes come from the registration, never from the
token.

Wait 30 seconds after registration before the first request. That is the
gateway's cache TTL.

## 4. Configure the client

Every SDK below signs the assertion itself, reuses it until shortly before it
expires, and sends only `Authorization` (no `X-API-Key`). All of them take the
same five inputs: the PEM, `client_id`, tenant, `kid` and audience
(`https://api.all-source.xyz` unless the gateway sets
`ALLSOURCE_ASSERTION_AUDIENCE` to something else).

### Rust

Feature `es256-assertion`. See [the Rust SDK README](../../sdks/rust/README.md).

```rust,ignore
let provider = allsource::Es256AssertionProvider::from_pkcs8_pem(
    &std::fs::read("client-key.pem")?,
    "my-service",
    "my-tenant",
    "registered-kid",
    "https://api.all-source.xyz",
)?;
let client = allsource::QueryClient::with_config_and_credential_provider(
    allsource::ClientConfig::new("https://api.all-source.xyz", ""),
    provider,
)?;
```

### TypeScript

See [the TypeScript SDK README](../../sdks/typescript/README.md).

```typescript
import { readFileSync } from "node:fs";
import { AllSourceClient, Es256AssertionProvider } from "@allsourcedev/client";

const credentials = await Es256AssertionProvider.fromPkcs8Pem(
  readFileSync("client-key.pem", "utf8"),
  {
    clientId: "my-service",
    tenant: "my-tenant",
    kid: "registered-kid",
    audience: "https://api.all-source.xyz",
  },
);
const client = new AllSourceClient({ baseUrl: "https://api.all-source.xyz", credentials });
```

### Python

Install the `assertion` extra (`pip install "allsource-client[assertion]"`). See
[the Python SDK README](../../sdks/python-client/README.md).

```python
from allsource_client import AllSourceClient, Es256AssertionProvider

provider = Es256AssertionProvider.from_pkcs8_pem(
    open("client-key.pem", "rb").read(),
    client_id="my-service",
    tenant="my-tenant",
    kid="registered-kid",
    audience="https://api.all-source.xyz",
)
client = AllSourceClient(base_url="https://api.all-source.xyz", credential_provider=provider)
```

### Go

See [the Go SDK README](../../sdks/go/README.md).

```go
pem, err := os.ReadFile("client-key.pem")
if err != nil {
	return err
}
provider, err := allsource.NewES256AssertionProvider(
	pem, "my-service", "my-tenant", "registered-kid", "https://api.all-source.xyz",
)
if err != nil {
	return err
}
client := allsource.New("", "https://api.all-source.xyz", allsource.WithCredentialProvider(provider))
```

### allsource-mcp (remote mode)

```bash
allsource-mcp --remote https://api.all-source.xyz
```

with these environment variables:

| Variable | Value |
|---|---|
| `ALLSOURCE_CLIENT_SIGNING_KEY` | the PEM itself, or a path to it |
| `ALLSOURCE_CLIENT_ID` | the registered `client_id` |
| `ALLSOURCE_TENANT_ID` | the tenant |
| `ALLSOURCE_CLIENT_KID` | the kid from registration |
| `ALLSOURCE_ASSERTION_AUDIENCE` | optional; defaults to `https://api.all-source.xyz` |

See [ALLSOURCE_MCP.md](ALLSOURCE_MCP.md#remote-gateway).

### Without an SDK

Mint the token yourself with any JOSE library:

- header: `{"alg":"ES256","typ":"JWT","kid":"<kid>"}`
- claims: `iss` = `client_id`, `sub` = tenant, `aud` = the audience as one
  string (an array is refused), `iat` = now, `exp` at most 60 s after `iat`,
  `jti` = a unique id per token
- signature: ES256 over the P-256 key, in JOSE (r || s) form, not DER

`jti` is logged for audit and not checked for reuse, so a token can be cached
and reused until shortly before `exp`. The Rust SDK reuses it until 15 seconds
before expiry.

Fail the service's boot when the key is missing or unreadable, rather than
starting without a credential.

## 5. Rotate

1. Generate a new keypair and have the operator register it (kid2).
2. Wait 30 seconds.
3. Switch every instance to the new PEM and kid2.
4. Wait 90 seconds (60 s assertion lifetime plus 30 s skew).
5. Have the operator revoke kid1 with
   `DELETE /api/v1/admin/tenants/:id/client-keys/:kid`.

A revocation is permanent: the same key cannot be registered again. On a leaked
key, revoke first and skip the waits. The full procedure is in
[CLIENT_KEY_ROTATION.md](../runbooks/CLIENT_KEY_ROTATION.md#rotate-kid1-to-kid2).

## 6. Troubleshoot

Every refusal has the body `{"error", "reason", "message"}`.

| reason | Status | Meaning | Fix |
|---|---|---|---|
| `asserted_signature` | 401 | malformed token or `kid`, missing `jti`, or the signature does not verify with the registered key | sign with the key that was registered; check the kid |
| `asserted_audience` | 401 | `aud` is not the single configured string | set the audience to the gateway's `ALLSOURCE_ASSERTION_AUDIENCE` |
| `asserted_skew` | 401 | `iat`/`nbf` more than 30 s in the future, or `exp - iat` over 60 s | fix the clock or the token lifetime |
| `asserted_expired` | 401 | `exp` missing or more than 30 s past | mint a fresh assertion |
| `asserted_unregistered` | 401 | no registration for (tenant, kid), or `iss`/`sub` mismatch | register the key, or fix `client_id` and tenant; wait 30 s after registering |
| `asserted_revoked` | 401 | the kid was revoked | rotate to a new key |
| `asserted_unavailable` | 503 | the gateway cannot read registrations and has no recent good read | retry after the `Retry-After` (5 s) |
| `scope` | 403 | the registration's scopes do not cover the route | register a key with the needed scopes |
| `legacy_key_sunset` | 401 | a managed onboarding key with no registration record, used after `ALLSOURCE_LEGACY_KEY_SUNSET` | move to a registered client key or a dashboard key |

The Query Service nests `reason` and `message` under `error` for
`legacy_key_sunset`. More detail: [TROUBLESHOOTING.md](TROUBLESHOOTING.md#client-assertion-and-legacy-key-refusals).
