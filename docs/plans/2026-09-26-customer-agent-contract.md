# AllSource customer investigation and review contract v1

Scope: `t-baaca8`, child of `t-70f73e`. This contract adds no route, MCP tool,
approval endpoint, deployment, paid entitlement or customer pilot. Runtime
readiness remains in `docs/CUSTOMER_AGENT_DELIVERY.md`.

## Existing implementation and boundaries

| Concern | Existing implementation | Binding for this contract |
|---|---|---|
| Durable data and metadata | Core WAL/Parquet and tenant system streams | Core remains the permitted shared datastore. No second database. |
| Tenant identity | Query Service `AuthPipeline`, `JwtAuth`, `TenantContext` | Resolve tenant and subject from verified credentials, never proposal fields. Prefer authoritative `tenant_id`, not a fallback unrelated record `id`. |
| API-key permissions | `ApiKeyCache` and `RustCoreClient.verify_api_key` | Existing generic auth is not a scoped customer-review grant. Access bead must prove live revocation, audience and tenant/object checks. |
| Projection definitions | `Projections.Catalog` | Validate target against existing curated templates; no customer code or reducer accepted. |
| Tenant projection configuration | `Projections.Enablement` | Recheck enabled set at preparation and human action; catalog membership alone is insufficient. |
| Read-only replay evidence | `Projections.ReplayAnalysis.analyze/2` | Reuse its 1,000-event sample and counts. `ready_to_replay` is not permission. Current total may be a fallback to sample size; do not assert exact total provenance. |
| Tenant rebuild execution | `ReplayController.create/2` → `TenantProjections.rebuild/2` | Future human action uses this tenant projection domain, with added receipt/auth boundary. Existing route behaviour is unchanged by this schema. |
| Other replay | Control Plane `StartReplayUseCase` → Core replay | Not a customer-review target. Do not expose global Core or infrastructure commands through this contract. |
| Agent-run timeline/comparison/restart proof | `t-e3d99f`; `docs/proposals/prd-agent-run-evidence-and-replay.md` | Reuse that domain when implemented. Do not create another comparator. Timestamp sorting and current projection readiness cannot prove authoritative event order or restart recovery. |
| Customer MCP transport | Existing `apps/mcp-server-elixir` | Future bounded tools call QS over the network. No imports across apps and no expansion of the separate Rust Prime server. |

## Typed request and evidence

`Domain.CustomerAgent.Proposal` accepts schema version 1 with exactly four fields:
`schema_version`, `kind`, `projection_name`, `sources`. JSON is capped at 64 KiB.
There are at most 32 sources. Each source has exactly `kind`, `ref`, `revision`
and `sha256`; revision is a positive JSON-safe integer, SHA-256 is 64 lower-case
hexadecimal characters, and reference is a 16–128 character opaque handle using
letters, digits, `_` or `-`. References are not URLs, file paths or raw event IDs.
Duplicate handles are rejected. Unknown fields and mismatched types are rejected
without echoing their values. No atom is created from customer input.

| Kind | Allowed sources | Target and missing evidence |
|---|---|---|
| `event_timeline` | `event_range`, `restart_evidence` | No projection target; absent event range remains unknown. |
| `run_comparison` | Up to two `run_evidence`, plus `restart_evidence` | First run is baseline, second candidate; no projection target; fewer than two run references means missing comparison source. |
| `replay_plan` | Up to one `replay_analysis`, plus `event_range` and `restart_evidence` | Curated projection name required; absent analysis remains unknown. |

Syntactically valid references are **unresolved**, not tenant-authorised or
source-verified. The access layer must resolve every handle under current tenant,
subject, grant and entitlement, pin its authoritative revision/content digest,
and invalidate review when anything changes. Source documents are data, never
instructions. Missing run ordering, restart evidence, comparison or total-count
provenance cannot become a pass, false boolean or zero count.

`Application.Services.CustomerAgentReview` validates projection names against
the real catalog and takes a disclosure-safe snapshot of the actual replay
analysis shape: counts, analysis scope, projection status, timestamp and explicit
unknowns. It omits entity IDs, event-type names, payloads, arbitrary warning text,
checks and `ready_to_replay`. Counts are non-negative JSON-safe integers; sample
size cannot exceed the existing 1,000-event bound. Missing fields are null.
Reported totals cannot be smaller than samples. This projection does not fetch
data or replace the source resolver.

Fingerprint v1 hashes a canonical array representation preserving source order.
It binds comparison direction, kind, target, reference, revision and digest; it is neither a
signature nor a human receipt. Future review digest must additionally bind the
resolved evidence snapshot, exact rendered proposal version, tenant, subject,
action, authority version and validity interval. An agent-supplied digest is
never trusted as proof that evidence is current.

## State, human authority and result ownership

Authority version: `allsource-operator-review-v1`.

The consequential action in v1 is **`start_tenant_projection_rebuild`** for one
enabled curated projection in the authenticated tenant. Preparation, comparison
and viewing are read-only. General infrastructure changes, global replay,
arbitrary queries, managed agents, ingestion, subscription changes and payments
are outside this gate. Existing SDK ingestion and ordinary existing API routes
remain unchanged.

Connector consent authorises narrowly described disclosure to a named host; it
does not approve a rebuild. OAuth/API keys and model assertions cannot be used as
human-action credentials. A future authenticated product browser gate must bind
current human identity, tenant membership, role, action, proposal version, source
versions, digest, expiry and a single-use receipt. It must independently enforce
CSRF/origin protection and deny all agent credentials, including bearer JWTs
that merely carry a user subject.

Existing roles are `admin`, `developer`, `readonly`, `serviceaccount` in Control
Plane `domain/entities/roles.go`. Scoped evidence preparation can be available to
tenant members granted read permission. The new rebuild gate starts with verified
tenant `admin` only; `developer`, `readonly`, `serviceaccount`, unrecognised roles
and unmapped roles cannot approve. This is a contract for the new surface, not a
change to existing role behaviour. Runtime must reconcile live membership and
permission source before release; token role claims alone are insufficient.
Query Service team management separately uses `admin`, `member`, `viewer`; that
vocabulary is not automatically equivalent to Control Plane roles. Resolve the
actual tenant membership source before connecting the gate.

`Domain.CustomerAgent.ReviewState` constructs only pending records from
server-supplied ownership and a digest. Lifetime is at most 24 hours. It reports
pending/rejected/approved, expiry at the exact deadline, supersession on changed
digest, and unavailable on missing evidence, clock rollback or unknown contract.
It cannot create an approval or execute an action. `approved` can only describe
a record loaded after the future human gate has verified and durably persisted
its receipt; this pure status calculation does not authenticate that record.
Rejection is terminal. Persisted historical decisions remain immutable; effective
approval expires or becomes superseded and cannot authorise later execution.

Results are owned by the source tenant and proposing subject, with explicitly
authorised tenant-operator access. Preparation IDs and result IDs are opaque,
never bearer capabilities. Every status/result retrieval repeats live ownership,
scope, revocation and entitlement checks. Delivery records refer to the exact
approved version and actual tenant replay ID. Timeout/uncertain execution remains
unknown until reconciled with that ID; retry cannot start another rebuild.

## Storage, disclosure, retention and costs

This implementation is pure and stores nothing. Production design uses Core for
minimal durable review metadata: opaque IDs, tenant/subject ownership, version,
digest, state, expiry and minimal receipt/result references. Do not write raw
event payloads, prompt/tool content or private documents into immutable review
events. Refer to existing authorised source objects; do not duplicate them.

Connection grants use separate admin-only Core system config records and
independent revocation markers, outside mutable tenant metadata. See
[the grant persistence boundary](2026-09-26-customer-agent-grants.md) for the
verified storage primitive and remaining runtime authorization requirements.

Pending review access expires within 24 hours or earlier source/grant expiry.
Deletion revokes handles and prevents all further retrieval; metadata audit
history is not falsely advertised as erased from WAL/Parquet. Source data follows
the tenant's actual configured plan/retention. Before production, the storage
bead must prove restart durability, receipt consumption, source expiry, deletion,
revocation and a documented audit-retention policy. No process-local map/ETS may
be the only copy of pending proposals, grants or action receipts.

The remote host receives the safe summary and explicitly authorised opaque
references only. Its own transcript retention, model processing and organisation
policies are separate from AllSource deletion. No claim of universal host data
deletion or zero training follows from this code. Before enabling a host, disclose
actual fields and processing path and verify its current account settings and
commercial rules. Founder-private or local-only data stays local unless a
specific processing amendment is authorised. Analytics, logs, URLs and tool
metadata must not contain proposal content, source references or credentials.

Cost envelope: 64 KiB request, 32 source handles, existing 1,000-event replay
sample, no model call, no new background execution and no rebuild from the agent
surface. Scope resolver/transport work must add bounded result bytes, per-tenant
rate/query quotas, concurrency and request deadlines before any endpoint opens.
Do not hide a full-event scan behind a sample label. Preserve live Indie catalog,
trial, plan limits and first-paid-renewal requirements; no new price or plan is
created here.

## Permitted work and release blockers

Permitted now: contract implementation, deterministic synthetic fixtures,
existing-domain integration tests and documentation. No second active customer
pilot or public connector follows from queue placement.

Release requires all of:

1. `t-7ce02d`: live identity/grant/tenant/object/entitlement checks, revoked and
   expired access denial, consent and cost limits.
2. `t-7b4430`: typed tools over existing MCP transport, durable preparation,
   result/status ownership and idempotency; no arbitrary query or executor.
3. `t-068e00`: product-only human authority, fresh evidence, one-use durable
   receipts and replay reconciliation; machine-credential denial.
4. `t-68d409`: authenticated product display/fallback with sources, unknowns,
   version, consequences and accessible human decisions.
5. `t-e3d99f`: actual run timeline/comparison/restart substrate where promised;
   missing capability must remain explicitly unavailable meanwhile.
6. `t-005d22` and `t-2ee18c`: complete customer skill, separate real-host proofs,
   distribution/privacy/security evidence and existing commercial rollout gate.

Synthetic fixtures prove schema behaviour only. They do not prove real customer
consent, tenant access, deployment, a payment, acceptance or product outcome.
