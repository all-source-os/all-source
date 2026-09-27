# Strict archive caller coverage

Source inspection at `99a161d68a91b71c310ce7d2bd2c87888f0bd6bd`.
This is a compatibility inventory, not a production traffic trace.

| Known caller | Tenant / persistence path | Strict archive admission |
| --- | --- | --- |
| Resend inbound webhook | Customer tenant resolved from recipient | Yes: first-ingest `expected_version=0` |
| Comms engagement recorder | `admin-comms` | Yes: first-ingest deduplication |
| Design partner submission and status changes | `admin-design-partners` | Yes: expected version |
| Partnership revisions | `admin-partnerships` | Yes: expected revision |
| Customer agent run adapter | Authorized customer tenant | Yes: conditional append and explicit retained-entity read |
| Better Auth AllSource client | Configured auth tenant | No: ordinary append/query, no expected version or integrity selector |
| Core tenant/config/auth/audit repositories and consumer cursor metadata | Reserved `_system` events in `SystemMetadataStore` | No: separate Core WAL and recovered metadata cache |

Evidence sources:

- `apps/control-plane/internal/interfaces/http/resend_webhook_handler.go`
- `apps/control-plane/internal/application/usecases/comms_audit.go`
- `apps/control-plane/internal/application/usecases/design_partners.go`
- `apps/control-plane/internal/application/usecases/partnerships.go`
- `apps/query-service/lib/query_service_ex/infrastructure/adapters/agent_run_store.ex`
- `crates/better-auth-allsource/src/client.rs`
- `apps/core/src/infrastructure/persistence/system_bootstrap.rs`
- `apps/core/src/infrastructure/persistence/system_store.rs`
- `apps/core/src/domain/value_objects/system_stream.rs`
- `apps/core/src/infrastructure/web/archive_work.rs`

The metadata audit's oversized directory is the ordinary tenant named `system`.
It is not the reserved `_system` metadata path. The audit only counted that
directory's filesystem entries and compressed bytes; it did not read its event
payloads or prove who wrote them. The Query Service's WebSocket service JWT uses
the name `system`, but that authentication claim alone does not identify the
producer or contents of the archived events.

Known Control Plane conditional callers use named admin tenants or customer
tenants. Core's separate metadata repositories do not traverse the audited
`system` Parquet directory for their WAL-backed operations. An unknown external
client could still send a conditional request for `system`; that would encounter
the configured strict admission limits. Do not infer an exhaustive production
caller inventory from the in-repository references.

Ordinary appends return before strict HTTP work admission, and requests without
the integrity selector retain their legacy query policy. That source boundary
does not prove latency under concurrent generic archive loading or establish
that every production path fits the new caps. In particular, arbitrary routed
customer email makes the dense non-system archive compatibility issue relevant
to existing traffic, even with customer agent features disabled.
