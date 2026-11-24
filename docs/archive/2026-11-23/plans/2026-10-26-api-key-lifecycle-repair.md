# Dashboard API key lifecycle repair

## Observed defect

The hosted sync qualification succeeded, but the dashboard's revoked proof credential still returned HTTP 200 from `https://api.all-source.xyz/api/v1/events/query`. A second read confirmed the result. Its JWT expiration is `2027-10-26T12:27:46Z`; the selected expiration displayed by the dashboard was `2026-11-25`.

The public health response identifies the tested ingress as `allsource-control-plane`. A Query Service-only repair cannot fix this observed ingress.

## Existing state and bypasses

- `ApiKeyController` issues a JWT with `is_api_key: true` and `sub` equal to the key ID. Its signing helper hardcodes a 365-day expiration. Requested scopes and expiration are recorded separately in lifecycle metadata.
- `ApiKeyStore` writes `apikey.created` and `apikey.revoked` to Core under entity `apikey:{tenant}`. Its ETS list cache is a projection, not an authorization authority.
- Control Plane's `AuthMiddleware` sends `ask_` keys through native Core key validation. Other tokens use `ValidateToken`, which checks signature and JWT expiration without consulting dashboard key lifecycle state.
- Query Service's Bearer and X-API-Key paths also validate these JWTs without checking current lifecycle metadata.
- Control Plane's short-lived `SignDelegationJWT` also sets `is_api_key: true`. Treating every such token as a dashboard key would break service delegation. `SignAPIKey` creates another long-lived legacy shape used by onboarding.

Core already owns the lifecycle events and provides tenant-scoped event queries. No new database or new public Core endpoint is necessary to read them. A repair must distinguish credential kinds explicitly rather than treating a token marker as proof of a durable registration.

## Proposed invariant

A managed dashboard key authenticates only while its authoritative tenant lifecycle record is active, in expiry, and permits the requested operation. Unknown registration, incomplete history, malformed state, or unavailable authority fails closed. Positive identity caches cannot override a later revoke or configured expiration.

Human sessions and short-lived service delegation keep their existing authority and must not be mistaken for dashboard credentials. Native `ask_` key behavior is a separate compatibility boundary.

## Repair boundaries

1. Define a managed-key claim and a service-delegation claim, including a migration rule for existing signed tokens. Bind managed key identity to its tenant and durable record.
2. Add bounded, authoritative lifecycle reads using the existing Core event API. Read complete history in deterministic order, fold creation/revocation, and reject incomplete or inconsistent results. Avoid the dashboard's indefinitely cached key list. Enforce the same contract at the actual Control Plane ingress and Query Service entry points.
3. Apply current metadata scopes and expiration during authorization. Respect requested expiration when minting new JWTs; reject invalid dates. Do not silently expand empty or unknown scope sets.
4. Make create/revoke return success only after durable acknowledgement. A failed durable write must not produce an apparently registered credential or a successful revocation UI state.
5. Exercise active, revoked, expired, unknown, unavailable-authority, pagination, and tenant-binding cases through the real middleware. Preserve human, service-delegation, and native-key tests. Add mint-expiration and create/revoke durability cases.

## Accepted compatibility policy

Unregistered long-lived API JWTs are rejected. This includes legacy onboarding tokens; their owners must obtain a newly registered credential. Human sessions, read-only view-as sessions and short-lived service delegation retain their separate authority. New onboarding credentials receive a distinct key ID and durable registration before the secret is returned. Native `ask_` validation remains a separate compatibility boundary.

New dashboard JWTs carry `credential_kind: managed_api_key`; new delegation JWTs carry `credential_kind: service_delegation`. Existing unnamed delegation tokens remain valid only with issuer `allsource`, positive issuance time and a lifetime no greater than 60 seconds. Explicit delegation has the same restrictions. A long-lived token cannot select delegation to avoid registration.

The dashboard's existing “Never expires” selection is supported through durable authority. When no expiration is requested, the issuer omits JWT `exp` and records null `expires_at`. Gateways accept that shape only for an explicitly marked managed key with an active registration whose expiration is also null. Explicit null, zero or malformed JWT `exp` is invalid. Human sessions and service delegation retain required finite expiration. Existing signed finite credentials keep their JWT deadline even when legacy metadata has no expiration.

Service delegation tolerates up to five seconds of peer issuance-clock skew while retaining the 60-second lifetime and strict expiration. Signed roles constrain operation scopes. The separate Control Plane admin middleware rejects programmatic credentials; Query Service's JWKS fallback uses the same lifecycle contract as its other JWT paths.

## Implementation contract

Both gateways use Core's existing `integrity=retained-entity-v1` read against `apikey:{tenant}`. This strict read includes archived history and binds its response to the tenant and entity. The authorization reader requires complete counts, no continuation, contiguous event versions and a matching final entity version. It accepts at most 1,000 events. A larger history, an unsupported integrity contract, malformed response or unavailable owner denies authorization. Tolerant paginated hot-store reads are unsuitable because an omitted archived revoke could grant access. Truncation must be tested as a denial, never as an active key.

Control Plane routes lifecycle reads to the same tenant home-region owner as writes and denies when a configured regional owner is unresolved. Query Service uses its configured write/leader client without follower routing or retries. Neither gateway uses the dashboard list cache as an authorization source.

Public event ingestion reserves the `apikey:` entity namespace and `apikey.` event types. This prevents event writers from manufacturing registration or revocation authority. Key-management handlers write these records internally. Managed credentials receive only explicitly supported operation scopes; unknown routes, scope names, MCP methods and MCP batches deny access. API-key management, configuration, billing and administrative operations require their existing non-managed authority.

The existing large Query Service HTTP adapter retains only a narrow network facade for strict key-history reads. Lifecycle folding and credential classification live in a separate adapter; request permissions live in a separate plug. This keeps the new security contract out of the adapter's unrelated network operations.

Native verification retains the original hosted-sync run and frozen Prime policy. The authorized authentication extension has its own accepted source plan and additional Go/Elixir check receipts. Passing the original Prime policy alone does not verify either gateway repair.

## Deployment and proof

Control Plane and Query Service must both deploy the agreed repair. No JWT signing-key rotation, broad credential revocation, tenant suspension, or unrelated operational change is included. Production deployment requires separate authorization after the source change and tests are reviewable.

Then repeat the same private, previously revoked proof credential against the actual public ingress and require HTTP 401. Run a new scoped credential through the existing Codex writer and Claude independent reader, revoke it, and require rejection. Verify configured expiry and scope enforcement. Keep all credential values outside source, logs, and memory.

Until that succeeds, hosted sync is verified but credential cleanup is not. The private proof credential must remain tracked as still usable; dashboard removal alone is not containment.
