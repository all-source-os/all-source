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

`Infrastructure.Adapters.CustomerAgentGrantStore` now implements issue, list,
credential verification and revoke through a conditional tenant registry at
`customer_agent_v2.connections.<sha256(tenant)>`. A 128-bit random handle and
256-bit secret form an `asreview_v2_` credential. Only its SHA-256 hash persists.
Grant, exact binding, consent and issuance accounting commit in one Core revision.
No secret returns before the mandatory conditional-write acknowledgement.

Consent `review-metadata-v1` identifies the selected host, permitted operations,
acceptance time and fields: workspace identity, membership role, MCP entitlement
and proposal validation. This is a recorded selection, not host attestation or
action approval. Missing, stale or mismatched consent denies use. Only context
and validation operations fit this consent; source data requires renewed consent.

The registry allows at most 16 unexpired, unrevoked connections per tenant and
64 issuances in a rolling 24 hours. These are abuse bounds, not pricing or seats.
Concurrent writers retry only Core's exact precondition conflict, at most four
attempts, with fresh revisions. Current-view receipts older than a day are pruned
on issuance; revocation/expiry does not replenish the rolling-day allowance.
Clock rollback cannot refill it. Encoded registry writes are capped at 60 KB.

Revocation writes an independent `customer_agent_v1.revoked.<random id>` record.
Any existing marker denies access, including malformed or false values. Only a
genuine missing-marker response permits credential verification to continue;
storage failures deny access. No deletion or reactivation is exposed by this
adapter. The marker commits before registry bookkeeping; a later bookkeeping
failure can report unavailable while the credential is already denied. Retrying
is safe while its receipt remains. Issuance always generates a fresh handle.

This supersedes the initial tenant-metadata implementation in `00f70228`.
Billing persists a complete tenant metadata map; a delayed write could restore
an older active grant after revocation. A regression reproduced that flaw.
Separate system records prevent billing writes from touching grants, and a late
registry rewrite cannot remove the independent revocation marker. Existing
Core config endpoints require Admin and acknowledge after system WAL persistence.
The service credential remains server-only; customers never receive it.
An administrator with arbitrary Core config access remains trusted and could
delete these records; this is not protection against a compromised administrator.

Every successful credential verification performs two current leader reads:
registry then revocation marker. The narrowly scoped registry methods derive
their key from a validated tenant and require Core's UUID revision. Revocation
methods restrict keys to the existing security namespaces. All use the configured
Core write URL with five-second request timeouts and no transport retries.
`get_tenant_for_authorization/1` uses the same leader client and
rejects invalid tenant path components. Ordinary read/write methods retain their
existing routing, timeouts and retries. No local credential cache or fallback
success is used. A failed or uncertain write returns failure.

No production grants were issued by the initial internal implementation.
This version deliberately does not read or migrate old tenant-metadata or
unconsented v1 credentials. They fail closed and require reconnect. The pure
binding schema remains v1; token and durable registry formats are v2.

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
inferred from a slug or a JWT role. New Control Plane OAuth workspaces now bind
the authenticated provider subject to a random workspace through a durable
conditional registry, then persist its initial owner. Email sign-in retains its
existing immutable auth-user-ID workspace binding and initializes the same
owner list. Neither path overwrites an existing member list or resets billing.
Legacy email-slug OAuth tenants receive no inferred owner. Incomplete legacy
email metadata and ownership migration remain unresolved. Team changes now
recheck current stored administration and use conditional revisions. Invitation
acceptance requires verified email, consumes its receipt alongside membership,
and durably selects the workspace for later sign-ins. The product has code
creation and authenticated join UI; no invitation email is sent. Legacy v1
invites require reissue. Both legacy member arrays and v2 member/receipt envelopes
are supported by current readers.
See [workspace provisioning evidence](../evidence/2026-09-27-customer-workspace-ownership/README.md).
See [team authority and browser evidence](../evidence/2026-09-27-team-membership/README.md)
for deployment order and the limits of ordinary session revocation.

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

The website now has `/dashboard/settings/connections` and a cookie-only proxy
for list/create/revoke. It verifies origin, rejects supplied Authorization,
forwards no unrelated cookies/query values, bounds bodies/timeouts, disables
caching/redirects, and returns fixed errors. The generic proxies cannot bypass
that route. Query Service independently verifies signed, verified-email product
sessions, denies API keys/demo/impersonation, derives tenant/subject/resource,
and rechecks current membership. Issuance additionally checks entitlement before
and after storing; billing expiry does not prevent owner revocation.
`CUSTOMER_CONNECTIONS_ENABLED` defaults off in both services and is evaluated
at runtime by the website. This form issues only the local Claude Code profile;
remote Claude must use its future PKCE flow. See the
[connection consent evidence](../evidence/2026-09-27-customer-connections/README.md).

1. **Transport and discovery.** Local stdio still needs real process/OS-owner
   binding and configuration delivery; a chosen client name is insufficient.
   Remote OAuth requires PKCE, exact redirects, resource binding and actual
   claude.ai verification. Both hosts remain required. Do not enable production
   issuance before those release gates pass.
2. **Complete live authority.** Current stored membership and entitlement are
   checked, with durable owner provisioning for new OAuth workspaces and the
   existing auth-service email identity binding, verified-email invitations and
   conditional member edits. Legacy OAuth ownership, production rollout
   and source ownership remain unresolved. The internal grant adapter's success
   alone is only credential verification. Neither that primitive nor the new
   access service grants human authority. Preserve the actual Indie catalog,
   trial and renewal rules while reconciling account onboarding.
3. **Storage and cost controls.** Current issuance counts and concurrency are
   durable and bounded. Final immutable-history/tombstone retention and deletion
   policy remains open; pruning is not erasure. HTTP input is bounded at 4 KiB
   for management and 64 KiB for tools before the generic parser, with no raw-body
   duplicate. The existing per-process rate limiter and fixed error responses
   are wired; protocol input and client results have size checks. These are not
   complete streaming-memory or distributed tool-cost controls. Lifetime system
   history is not capped by rolling-day issuance limits. Revocation disables access;
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
   processing. Ordinary team session switching is implemented, but no proposal
   store, consequential-action human receipt, review display or production
   deployment is included here.

No customer credential was minted against production, no private source was
retrieved, and no second pilot was activated. The task's acceptance criteria stay
unchecked until the full access path is verified.

[Grant isolation and real-Core recovery evidence](../evidence/2026-09-26-customer-agent-grant-isolation/README.md).
