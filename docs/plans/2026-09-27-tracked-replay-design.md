# Durable identity for tenant projection replay

## Decision and scope

Use Core conditional configuration for a minimal execution journal, and reuse the
existing Query Service `TenantProjections` engine. Reserve one operation identity
before dispatch. Only the caller receiving the successful absent-key write may
dispatch. A retry reads the same record; it never dispatches again. A lost write
acknowledgement deliberately cannot authorize execution.

This is an internal foundation for the consequential human gate in
`2026-09-26-customer-agent-contract.md`. It is not an approval receipt, source
snapshot, public endpoint, or authentication mechanism. Ordinary replay endpoints
and ingestion behavior stay unchanged. No customer feature flag is enabled.

Alternatives rejected: an ETS-only deduplication map loses identity on restart;
automatic redispatch after a timeout can replace projection state twice. A second
database or replay engine would violate the existing architecture.

## Identity, publication, and recovery

Each record binds tenant, operation UUID, curated projection, protocol version,
request digest, one replay ID, and one cutoff. An operation reused for a different
projection conflicts. Core keys hash tenant and operation; stored metadata has no
event payload, credentials, exception messages, or customer prose.

The journal initially reports `unknown`: reservation alone proves neither dispatch
nor completion. The existing engine supplies live `running` status while it owns
the job. It folds in shadow state and publishes through its existing generation
pointer. Only after publication can the journal record `completed`. Failure and
cancellation preserve the previous generation. Final writes use Core revision CAS;
matching terminal retries succeed, conflicting terminal writes fail.

The engine persists terminal evidence asynchronously so a Core outage cannot stall
every tenant's GenServer. A read retries finalization from an extant terminal job.
If Query Service dies before durable finalization, status stays `unknown`, with the
original replay ID. It must not say failed, completed, or start a replacement.
Completed journal records survive Core and Query Service restart. They describe a
historical operation, not the continued presence of an ephemeral read model.

## Verification and remaining gate

Use actual Core WAL/conditional config for competing starts, hard restart,
terminal recovery, changed-operation conflicts, tenant isolation, cancellation,
and publication before success. Inject only projection source data to make fold
timing deterministic; do not substitute a fake journal for durability evidence.
HTTP tests cover malformed acknowledgements, response bounds and no redirects.

Before a customer action can use this foundation, complete current human-role
checks, exact source/content approval, one-use approval consumption, expiry,
source revalidation, pinned replay inputs, metering and product controls. Core's
current `retained-entity-v1` contract is entity-only and cannot prove a whole tenant
snapshot. Ordinary paginated replay and a 1,000-event preview must not be presented
as exact approved source integrity. Unknown executions need explicit reconciliation
evidence; this increment provides no automatic recovery dispatch.

Sources: `apps/query-service/lib/query_service_ex/projections/tenant_projections.ex`,
`apps/query-service/lib/query_service_ex/infrastructure/adapters/customer_review_store.ex`,
`apps/core/src/infrastructure/web/retained_query.rs`, and
`docs/plans/2026-09-26-customer-agent-contract.md`.
