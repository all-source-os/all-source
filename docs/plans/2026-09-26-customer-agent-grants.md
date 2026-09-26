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
   and retention/tombstone policy before exposing issuance. System record growth
   is currently not capped by this internal adapter. Revocation disables access;
   it does not erase immutable Core audit history. Local real-Core SIGKILL/restart
   tests now prove grant/revocation WAL recovery and denial after stale tenant
   and grant writes. They do not prove replication, failover, production topology
   or atomicity with a later data disclosure. Recheck before disclosure and prove
   the deployment's consistency guarantees before exposing the flow.
4. **Tool/resource enforcement.** Recheck each invocation and result disclosure,
   deny agent credentials at the separate product human gate, bind object
   versions and ownership, and prove reconnect/host failure paths through the
   actual MCP process. No new tool, resource, route, user session, proposal store,
   human receipt or deployment is included here.

No customer credential was minted against production, no private source was
retrieved, and no second pilot was activated. The task's acceptance criteria stay
unchecked until the full access path is verified.

[Current isolation and real-Core recovery evidence](../evidence/2026-09-26-customer-agent-grant-isolation/README.md).
