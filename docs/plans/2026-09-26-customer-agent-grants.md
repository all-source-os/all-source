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
credential verification and revoke operations through existing Core tenant
metadata. It generates a 128-bit random handle and a 256-bit random secret;
only SHA-256 of the complete opaque credential is stored. The one-time plaintext
has an `asreview_v1_` prefix and is structurally distinct from a user/API-key JWT.
No secret is returned if Core fails to acknowledge storage.

Metadata path: `customer_agent_v1.grants.<random id>`, containing version,
binding, allowed operations, validity interval, active status and token hash.
Revocation deep-merges `active: false` plus revocation time. Sibling tenant
metadata is preserved. A failed revoke returns failure; there is no local cache
or fallback success. Revocation is idempotent. Nothing can rotate/reactivate an
existing credential through this adapter; issuance generates a fresh handle.

Every credential verification fetches current metadata. The new
`RustCoreClient.get_tenant_for_authorization/1` routes to configured Core write
URL, not healthy followers, with a five-second timeout and no retry. Invalid
tenant path components are rejected. Existing ordinary read/write methods retain
their existing routing, timeouts and retries. Grant writes currently use the
existing metadata patch method (30-second timeout, up to three retries); the
same idempotent partial update is retried, not a newly generated grant.

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

The opaque credential fails the existing API-key JWT verifier. That does not
prove every other legacy endpoint rejects malformed credentials gracefully;
transport integration must test that independently. No endpoint currently
accepts these new grants.

## Work still required before use

1. **Issue/revoke authority and discovery.** Wire primitives through the existing
   MCP/product runtime. Authenticate issuance/revocation outside this adapter;
   its arguments are trusted server context, not permission to expose these
   methods directly. Add product connection UI, explicit host/field consent,
   per-client restrictions and discovery. Remote OAuth, if chosen, needs PKCE and
   exact redirect validation. Local stdio needs real process/owner binding.
2. **Live eligibility.** After credential verification, independently check
   current membership/role, entitlement and source ownership, before retrieval
   and before disclosure. This adapter's success is only credential verification.
   It grants no paid bypass or human action. QS team roles (`admin`, `member`,
   `viewer`) and Control Plane roles (`admin`, `developer`, `readonly`,
   `serviceaccount`) differ; resolve authoritative live membership, not a JWT
   string or a fabricated mapping. Billing status fallback to free/active is not
   paid evidence; preserve actual Indie catalog/trial/renewal rules.
3. **Storage and cost controls.** Add authoritative per-tenant grant count,
   request/rate/concurrency limits, body and response bounds, credential redaction
   and retention/tombstone policy before exposing issuance. Metadata growth is
   currently not capped by this internal adapter. Revocation disables access; it
   does not erase immutable Core audit history. Prove a real Core restart and
   failover; HTTP fixtures are not WAL/recovery proof.
4. **Tool/resource enforcement.** Recheck each invocation and result disclosure,
   deny agent credentials at the separate product human gate, bind object
   versions and ownership, and prove reconnect/host failure paths through the
   actual MCP process. No new tool, resource, route, user session, proposal store,
   human receipt or deployment is included here.

No customer credential was minted against production, no private source was
retrieved, and no second pilot was activated. The task's acceptance criteria stay
unchecked until the full access path is verified.
