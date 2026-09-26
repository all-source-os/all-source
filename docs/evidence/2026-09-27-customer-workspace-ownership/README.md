# Durable customer workspace ownership

Progress on `t-7ce02d`; the scoped connection task remains open. This change
establishes initial owner records needed by live MCP membership checks. It does
not expose customer grant issuance, consent, source retrieval or a human gate.

## Runtime changes

Core adds admin-only `POST /api/v1/config/conditional/set`. Its mandatory
condition is either `{"kind":"absent"}` or
`{"kind":"revision","revision":"<UUID>"}`. Revision is the durable event ID,
returned by config reads and writes and preserved on recovery. Conditional,
unconditional and delete operations share one repository writer lock; mismatches
return 409 without appending an event. Event IDs reject stale observations even
after a value is restored or a key is deleted and recreated. The distinct route
prevents old Core versions from silently ignoring a condition, and its extra
path segment preserves access to an existing config key named `conditional`.

Tenant creation accepts an optional metadata object, bounded to 32 KiB. When
present, the complete validated tenant is initialized in one durable creation
event. Concurrent creation has one winner; a duplicate cannot reset a paid
subscription. Event-sourced and in-memory repositories implement this operation.
Other repository implementations fail closed rather than doing create-then-save.
Tenant mutations now join config mutations in Core's follower write guard.

After successful provider authentication, normal OAuth provisioning first writes
`customer_agent_v1.workspace.<sha256(subject)>` conditionally. Its versioned
record binds the exact provider subject to a cryptographically random `ws-…`
tenant ID. Concurrent callbacks re-read the winning record. Tenant creation
stores canonical trial subscription and quota metadata atomically, then the
initial `team:<tenant>:members` record is created only if absent. A session is
signed only after a fresh read finds exactly one matching admin/member. Failed
writes deny the session; retries resume the registered workspace after restart.

Email authentication through `AUTH_SERVICE_URL` retains the existing
`email-<immutable auth user ID>` tenant mapping. It uses the same atomic tenant
initialization and conditional member initialization. An email match does not
join an OAuth workspace or grant an `ADMIN_EMAILS` global role. Product JWTs
remain separate from the auth service's opaque session. Existing member roles,
member removal and paid metadata survive subsequent logins.

Core calls have a 30-second overall context and a 64 KiB response limit; errors
do not include upstream bodies. Stored metadata is re-read before ownership is
initialized. Missing/malformed subscription or quota objects deny provisioning.
No unconditional metadata repair runs during login.
Only the exact expected conditional/duplicate conflicts are accepted for a
re-read; follower conflicts deny even an already registered owner's login.

## Evidence and limits

[Verification record](verification.md) records exact commands and outcomes.
Tests use synthetic identities and real local Core processes, including hard
restart against the same WAL. They do not authenticate against Google/GitHub or
the production email service. No production customer credentials were issued.

- Deploy Core before Control Plane. Older Core lacks the conditional capability;
  the client refuses fallback to unsafe unconditional writes. No deployment is
  claimed by this local evidence.
- Legacy OAuth email-slug tenants keep their existing login mapping. This change
  does not establish their owner; new MCP eligibility still requires explicit
  stored membership. Cross-provider account linking is not added.
- Legacy email tenants lacking valid initial subscription/quota objects need
  reviewed migration; login refuses them instead of overwriting uncertain
  billing state. Their auth accounts remain saved.
- Invitation acceptance and team edits still need authoritative conditional
  lifecycle handling. This proof does not cover those existing paths.
- Conditional serialization is within one leader process/repository. Replication,
  failover and authorization-plus-disclosure atomicity are unproven. Other
  legacy tenant save/metadata paths are not all serialized with initial creation.
  Existing billing concurrency work remains tracked separately.
- Administrative config access remains trusted. Deleting an entire owner list
  can permit initialization again; normal member removal persists an empty list
  and is verified to deny login. No protection against a compromised Core admin
  is claimed.
- Current trial limits remain 1,000 events / 100 queries and 14 days. No MCP scope
  is invented; persisted scope is still required by the separate access policy.
- Grant issuance/revocation UI, durable host/field consent, transport ownership,
  retention/count/concurrency controls, sources, full preparation and human
  approval/display remain open. Task acceptance criteria remain unchecked.
- Optional native Claude verification remains unrun: automatic approval review
  rejected external skill/synthetic-context processing pending user permission.
  No external model call or charge occurred.
