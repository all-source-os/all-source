# Customer connection grants: persistence and verification

Progress on `t-7ce02d`; **not completed access or a released MCP connection**.
Extends [the v1 customer contract](2026-09-26-customer-agent-contract.md).

## Implemented boundary

`Domain.CustomerAgent.ConnectionGrant` defines a separate operation vocabulary:
`read_context`, `validate_proposal`, `prepare_proposal`, `read_review`,
`read_result`. It rejects approval/execution operations and duplicate scopes.
Bindings require tenant, subject, client and exact HTTPS resource (no credentials,
query or fragment). Lifetime is at most 24 hours; clock rollback and exact expiry
deny use. The schema is `allsource-customer-grant-v1`.

`Infrastructure.Adapters.CustomerAgentGrantStore` implements internal issue,
credential verification and revoke operations through existing admin-only Core
system config records. It generates a 128-bit random handle and a 256-bit random secret;
only SHA-256 of the complete opaque credential is stored. The one-time plaintext
has an `asreview_v1_` prefix and is structurally distinct from a user/API-key JWT.
No secret is returned if Core fails to acknowledge storage.

Grant key: `customer_agent_v1.grant.<random id>`, containing version,
binding, allowed operations, validity interval, active status and token hash.
Revocation writes an independent `customer_agent_v1.revoked.<random id>` record.
Any existing marker denies access, including malformed or false values. Only a
genuine missing-marker response permits credential verification to continue;
storage failures deny access. No deletion or reactivation is exposed by this
adapter. Repeated revocation is idempotent; issuance generates a fresh handle.

This supersedes the initial tenant-metadata implementation in `00f70228`.
Billing persists a complete tenant metadata map; a delayed write could restore
an older active grant after revocation. A regression reproduced that flaw.
Separate system records prevent billing writes from touching grants, and a late
grant-record rewrite cannot remove the independent revocation marker. Existing
Core config endpoints require Admin and acknowledge after system WAL persistence.
The service credential remains server-only; customers never receive it.
An administrator with arbitrary Core config access remains trusted and could
delete these records; this is not protection against a compromised administrator.

Every successful credential verification performs two current leader reads:
grant then revocation marker. `RustCoreClient.get_config_for_authorization/1`
and `put_config_for_authorization/2` restrict keys to these two namespaces,
use the configured Core write URL and apply a five-second timeout per request
with no retries. `get_tenant_for_authorization/1` uses the same leader client and
rejects invalid tenant path components. Ordinary read/write methods retain their
existing routing, timeouts and retries. No local credential cache or fallback
success is used. A failed or uncertain write returns failure.

No production grants were issued by the initial internal implementation.
This version deliberately does not read or migrate its old tenant-metadata
records; there is no legacy fallback. The schema/token prefix stays v1 because
the customer connection has not been released.

Credential checks use a constant-time hash comparison and exact binding/scope
checks. Wrong tenant, subject, client, resource, operation, missing/revoked grant,
tampered secret or expired credential denies use. Core failures yield a fixed
unavailable error with no response-body disclosure. Returned internal context
uses a field allowlist and excludes the hash and arbitrary metadata.

## Why existing generic key verification is insufficient here

`RustCoreClient.verify_api_key/1` checks JWT signature/expiry and fetches a tenant.
It does not read `ApiKeyStore` revocation records. `ApiKeyStore` also has a local
read cache and acknowledges some failed durable writes locally. Normal tenant
reads may use followers. The new customer review path must not interpret any of
those existing checks as immediate revocation, host-specific consent or human
authority. This change leaves existing generic API-key behaviour unchanged.

The opaque credential fails the existing API-key JWT verifier. The restricted
HTTP routes now accept these grants and reject a synthetic administrator JWT.
That does not prove every other legacy endpoint rejects malformed credentials
gracefully; those generic routes are not changed here.

## Restricted runtime and live eligibility

The existing Query Service now serves opt-in `POST /api/customer-agent/context`
and `POST /api/customer-agent/validate`. They bypass the generic JWT/dev/cached
tenant pipelines and require a separate opaque grant and exact configured
resource. The application uses an access port; its infrastructure adapter reads
current grant/revocation records, tenant metadata and the Control Plane member
list from Core's leader. It verifies the grant again after eligibility reads,
accounts for elapsed time before returning, and repeats access verification
before sending the HTTP result. These are sequential reads, not a transactional
snapshot or atomic authorization-plus-disclosure guarantee.

Membership requires one exact stored subject with `admin` or `member` role in
`team:<tenant>:members`. Missing/duplicate membership and unknown roles deny
access. Actual `oauth:<provider>:<id>` subjects are accepted as opaque identities;
tenant/client path components retain the stricter character set. No owner is
inferred from a slug or a JWT role. Normal Control Plane OAuth registration does
not currently persist an owner in this list; owner bootstrap remains unresolved.

Eligibility requires an active, non-demo tenant and explicit persisted MCP scope.
It preserves current `active`, `on_trial`, `trialing` and `past_due` statuses,
including existing dunning grace. Trial access requires a future stored trial
expiry; converted paid subscriptions ignore historical trial dates. A stored
subscription end remains a hard deadline. Unknown or exhausted query budgets
deny access; the existing `-1` unlimited convention is preserved. No catalog
price, new scope, overage charge or entitlement is inferred from the tier name.
Current agent-trial metadata lacks an MCP scope and therefore cannot satisfy
this policy without an explicit product entitlement decision.

The existing Elixir MCP stdio server has an exclusive customer review profile.
It exposes only `allsource_review_context` and
`allsource_validate_review_proposal`, using the HTTP routes above. Generic Core,
admin, resource and prompt operations are unavailable in this profile, even if
the existing system-admin environment flag is set. No Core backend or websocket
is started for it. Tool schemas, structured results and matching JSON text
describe the actual limited behavior:

- Context: `eligibility_verified`, `source_access: unresolved`, preparation
  unavailable and human approval required in the product.
- Validation: `valid_unresolved`, request fingerprint and explicit unknowns,
  `persisted: false`, `approved: false`.

Neither result creates a pending review or reads source data. Connection
configuration stays outside tool arguments. See the
[runtime evidence and configuration boundary](../evidence/2026-09-26-customer-agent-live-access/README.md).

## Work still required before use

1. **Issue/revoke authority and discovery.** Wire primitives through the existing
   product runtime. The restricted MCP bindings exist, but issuance/revocation
   still needs authentication outside this adapter;
   its arguments are trusted server context, not permission to expose these
   methods directly. Add product connection UI, explicit host/field consent,
   per-client restrictions and discovery. Remote OAuth, if chosen, needs PKCE and
   exact redirect validation. Local stdio needs real process/owner binding.
2. **Complete live authority.** Current stored membership and entitlement are
   checked, but normal-account owner provisioning, explicit host/field consent
   and source ownership remain unresolved. The internal grant adapter's success
   alone is only credential verification. Neither that primitive nor the new
   access service grants human authority. Preserve the actual Indie catalog,
   trial and renewal rules while reconciling account onboarding.
3. **Storage and cost controls.** Add authoritative per-tenant grant count,
   concurrency limits and retention/tombstone policy before exposing issuance.
   HTTP input is bounded at 64 KiB before the generic parser, with no raw-body
   duplicate. The existing per-process rate limiter and fixed error responses
   are wired; protocol input and client results have size checks. These are not
   complete streaming-memory or distributed concurrency/cost controls. System
   record growth is currently not capped by the internal adapter. Revocation disables access;
   it does not erase immutable Core audit history. Local real-Core SIGKILL/restart
   tests now prove grant/revocation WAL recovery and denial after stale tenant
   and grant writes. They do not prove replication, failover, production topology
   or atomicity with a later data disclosure. Recheck before disclosure and prove
   the deployment's consistency guarantees before exposing source data.
4. **Tool/resource enforcement.** Recheck each invocation and result disclosure,
   deny agent credentials at the separate product human gate, bind object
   versions and ownership. Revocation and reconnect denial are now proven through
   the compiled MCP process and real HTTP/Core stack. Native Claude host proof is
   separate and still pending; the optional test requires approval for external
   processing. No user session, proposal store, human receipt, product display
   or deployment is included here.

No customer credential was minted against production, no private source was
retrieved, and no second pilot was activated. The task's acceptance criteria stay
unchecked until the full access path is verified.

[Grant isolation and real-Core recovery evidence](../evidence/2026-09-26-customer-agent-grant-isolation/README.md).
