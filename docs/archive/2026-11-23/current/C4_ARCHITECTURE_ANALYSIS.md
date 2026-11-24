# AllSource C4 Architecture Analysis

> **Updated 2026-03-01**: Corrected to reflect v0.10.0+ architecture. Query Service is stateless (no PostgreSQL). Tenants are managed by Core via event-sourced system streams (WAL-durable). Control Plane delegates to Core. MCP Server is Elixir (port 4000). See ADR-005 for PostgreSQL removal decision.

## C4 Level 1 — System Context

```
┌─────────────────────────────────────────────────────────────────────┐
│                         EXTERNAL ACTORS                             │
│                                                                     │
│  [App Developer]    [Platform Admin]    [Billing System]            │
│   Uses events API    Manages tenants     LemonSqueezy webhooks      │
│   via Query Service  via Control Plane   via Query Service          │
└────────┬───────────────────┬───────────────────┬────────────────────┘
         │                   │                   │
         ▼                   ▼                   ▼
┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
│  Query Service  │ │  Control Plane  │ │   MCP Server    │
│  (Elixir:3902)  │ │  (Go:3901)      │ │  (Elixir:4000)  │
│                 │ │                 │ │                 │
│ API Gateway     │ │ Admin Console   │ │ LLM Tool Use    │
│ Auth (OAuth)    │ │ Operations      │ │                 │
│ Billing/Quotas  │ │ RBAC Policies   │ │                 │
│ Tenant Scoping  │ │ Audit Trail     │ │                 │
└────────┬────────┘ └────────┬────────┘ └────────┬────────┘
         │                   │                   │
         └───────────┬───────┴───────────────────┘
                     ▼
          ┌─────────────────────┐
          │    AllSource Core   │
          │    (Rust:3900)      │
          │                     │
          │  Event Store (WAL)  │
          │  Parquet + DashMap  │
          │  + Tenant/Audit/    │
          │    Config metadata  │
          └─────────────────────┘
```

---

## C4 Level 2 — Container Diagram

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        AllSource Platform                               │
│                                                                         │
│  ┌──────────────────────┐          ┌──────────────────────┐            │
│  │   Query Service      │          │   Control Plane      │            │
│  │   (Elixir/Phoenix)   │          │   (Go/Gin)           │            │
│  │                      │          │                      │            │
│  │ ┌──────────────────┐ │          │ ┌──────────────────┐ │            │
│  │ │ OAuth (Google/GH)│ │          │ │ JWT Auth + RBAC  │ │            │
│  │ │ JWT (Guardian)   │ │          │ │ (4 roles)        │ │            │
│  │ └──────────────────┘ │          │ └──────────────────┘ │            │
│  │ ┌──────────────────┐ │          │ ┌──────────────────┐ │            │
│  │ │ Tenant Context   │◄┼── GAP ──┼►│ Tenant CRUD      │ │            │
│  │ │ (stateless)      │ │  No sync │ │ (delegates Core) │ │            │
│  │ └──────────────────┘ │          │ └──────────────────┘ │            │
│  │ ┌──────────────────┐ │          │ ┌──────────────────┐ │            │
│  │ │ Usage Metering   │ │          │ │ Policy Engine    │ │            │
│  │ │ Quota Enforce    │ │          │ │ (in-memory)      │ │            │
│  │ │ Billing (Lemon)  │ │          │ └──────────────────┘ │            │
│  │ └──────────────────┘ │          │ ┌──────────────────┐ │            │
│  │ ┌──────────────────┐ │          │ │ Operations Mgmt  │ │            │
│  │ │ Consistency      │ │          │ │ Snapshot/Compact │ │            │
│  │ │ Routing          │ │          │ │ Replay/Backup    │ │            │
│  │ │ (leader/follower)│ │          │ └──────────────────┘ │            │
│  │ └──────────────────┘ │          │ ┌──────────────────┐ │            │
│  │ ┌──────────────────┐ │          │ │ Audit Trail      │ │            │
│  │ │ Analytics Engine │ │          │ │ (delegates Core) │ │            │
│  │ └──────────────────┘ │          │ └──────────────────┘ │            │
│  └──────────┬───────────┘          └──────────┬───────────┘            │
│             │                                 │                        │
│             │  HTTP (events, queries,         │  HTTP (tenants, audit, │
│             │  projections, schemas)          │  config, operations)   │
│             │                                 │                        │
│             └────────────┬────────────────────┘                        │
│                          ▼                                             │
│             ┌──────────────────────┐                                   │
│             │    AllSource Core    │                                   │
│             │    (Rust/Axum)       │                                   │
│             │                     │                                   │
│             │  Event Data:        │    Metadata (event-sourced):      │
│             │  ├─ WAL (durability)│    ├─ /api/v1/tenants   ◄─ CP    │
│             │  ├─ Parquet (cold)  │    ├─ /api/v1/audit     ◄─ CP    │
│             │  ├─ DashMap (hot)   │    ├─ /api/v1/config    ◄─ CP    │
│             │  │                  │    ├─ /api/v1/auth      ◄─ CP    │
│             │  ├─ Events API      │    └─ /api/v1/stats            │
│             │  ├─ Projections     │                                   │
│             │  ├─ Schemas         │    Ops:                           │
│             │  ├─ Snapshots       │    ├─ /api/v1/compaction ◄─ CP   │
│             │  ├─ Pipelines       │    ├─ /api/v1/replay     ◄─ CP   │
│             │  └─ Analytics       │    └─ /api/v1/snapshots  ◄─ CP   │
│             └──────────────────────┘                                   │
│                                                                         │
│  NOTE: No PostgreSQL in v0.10.0+. Query Service is stateless.           │
│  Tenants: Core event-sourced system streams (WAL-durable) + DashMap    │
│  cache. CP delegates to Core. Users: Core DashMap (in-memory).         │
│  Future: PostgreSQL may be added for billing/subscription metadata     │
│  only. See ADR-005 and TENANT_ARCHITECTURE.md.                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## C4 Level 3 — Component Diagram: Responsibility Overlap & Gaps

```
                    TENANT LIFECYCLE (THE BIG GAP)
    ╔═══════════════════════════════════════════════════════════╗
    ║                                                           ║
    ║   Core (DashMap)              Control Plane (in-memory)   ║
    ║   ─────────────────          ──────────────────────      ║
    ║   Tenant model:              Tenant model:               ║
    ║   • TenantId, Name           • ID, Name, Description    ║
    ║   • Status (active/susp)     • Status (active/susp/del) ║
    ║   • Metadata map             • Metadata map             ║
    ║   • Quotas, rate limits      • CreatedAt, UpdatedAt     ║
    ║   • CreatedAt, UpdatedAt                                 ║
    ║                              NO billing fields           ║
    ║   NO billing fields          NO usage tracking            ║
    ║   NO subscription tier       NO subscription tier        ║
    ║                                                           ║
    ║   ⚠️  BOTH ARE IN-MEMORY (lost on restart for metadata)  ║
    ║   ⚠️  CP delegates to Core, but no canonical persistent   ║
    ║      store for tenant billing/subscription data           ║
    ╚═══════════════════════════════════════════════════════════╝

                    AUTH (THE SECOND GAP)
    ╔═══════════════════════════════════════════════════════════╗
    ║                                                           ║
    ║   Query Service              Control Plane     Core      ║
    ║   ─────────────              ──────────────    ────      ║
    ║   OAuth (Google/GH)          JWT validation    Auth API  ║
    ║   JOSE JWT (HS256)           own JWT signing   register  ║
    ║   stateless (no user DB)     proxies to Core   login     ║
    ║   delegates to CP for auth   RBAC (4 roles)    API keys  ║
    ║                                                           ║
    ║   ⚠️  THREE separate auth systems, no SSO                 ║
    ║   ⚠️  QS users ≠ CP users ≠ Core users                   ║
    ║   ⚠️  JWT secrets may differ between services             ║
    ╚═══════════════════════════════════════════════════════════╝
```

---

## DBaaS Function Map — Who Owns What

```
┌────────────────────────┬──────────┬──────────┬──────────┬──────────┐
│ DBaaS Function         │   Core   │ Query Svc│ Ctrl Pln │  STATUS  │
├────────────────────────┼──────────┼──────────┼──────────┼──────────┤
│ EVENT STORAGE          │          │          │          │          │
│ ├─ Ingest events       │    ✅    │  proxy   │          │ GOOD     │
│ ├─ Query events        │    ✅    │  proxy   │          │ GOOD     │
│ ├─ Batch ingest        │    ✅    │  proxy   │          │ GOOD     │
│ ├─ Event streaming     │    ✅    │  proxy   │          │ GOOD     │
│ └─ WAL/Parquet durable │    ✅    │          │          │ GOOD     │
├────────────────────────┼──────────┼──────────┼──────────┼──────────┤
│ DATA FEATURES          │          │          │          │          │
│ ├─ Projections         │    ✅    │  proxy   │          │ GOOD     │
│ ├─ Schemas             │    ✅    │  proxy   │  proxy   │ GOOD     │
│ ├─ Snapshots           │    ✅    │  proxy   │  proxy   │ GOOD     │
│ ├─ Analytics           │    ✅    │ extended │          │ GOOD     │
│ └─ Pipelines           │    ✅    │          │          │ GOOD     │
├────────────────────────┼──────────┼──────────┼──────────┼──────────┤
│ MULTI-TENANCY          │          │          │          │          │
│ ├─ Tenant CRUD         │    ✅    │  proxy   │  ✅→Core │ ⚠️ DUAL  │
│ ├─ Tenant isolation    │  by key  │  by ctx  │          │ GOOD     │
│ ├─ Tenant billing      │          │    ✅    │          │ QS only  │
│ ├─ Tenant quotas       │  stores  │ enforces │  proxy   │ SPLIT    │
│ ├─ Tenant stats        │    ✅    │          │  proxy   │ GOOD     │
│ └─ Tenant suspend/del  │    ✅    │          │    ✅    │ ⚠️ DUAL  │
├────────────────────────┼──────────┼──────────┼──────────┼──────────┤
│ AUTH & ACCESS           │          │          │          │          │
│ ├─ User registration   │    ✅    │  proxy   │          │ OK       │
│ ├─ OAuth (social)      │          │    ✅    │          │ QS only  │
│ ├─ API keys            │    ✅    │          │          │ Core     │
│ ├─ JWT issuance        │          │    ✅    │    ✅    │ ⚠️ DUAL  │
│ ├─ RBAC enforcement    │   basic  │          │    ✅    │ CP only  │
│ └─ Policy engine       │          │          │    ✅    │ CP only  │
├────────────────────────┼──────────┼──────────┼──────────┼──────────┤
│ OPERATIONS             │          │          │          │          │
│ ├─ Compaction          │    ✅    │          │  trigger │ GOOD     │
│ ├─ Snapshot creation   │    ✅    │          │  trigger │ GOOD     │
│ ├─ Replay              │    ✅    │          │  trigger │ GOOD     │
│ ├─ Backup              │          │          │  ✅(API) │ ⚠️ STUB? │
│ └─ Operation history   │          │          │  in-mem  │ ⚠️ LOST  │
├────────────────────────┼──────────┼──────────┼──────────┼──────────┤
│ OBSERVABILITY          │          │          │          │          │
│ ├─ Health checks       │    ✅    │    ✅    │    ✅    │ GOOD     │
│ ├─ Prometheus metrics  │    ✅    │    ✅    │    ✅    │ GOOD     │
│ ├─ Audit trail         │    ✅    │          │  ✅→Core │ GOOD     │
│ ├─ OpenTelemetry       │          │          │    ✅    │ CP only  │
│ └─ Cluster health      │  /health │  checks  │  agg.   │ GOOD     │
├────────────────────────┼──────────┼──────────┼──────────┼──────────┤
│ CLUSTER MANAGEMENT     │          │          │          │          │
│ ├─ Leader election     │ internal │          │          │ Core     │
│ ├─ Failover notify     │          │ receives │          │ QS       │
│ ├─ Read routing        │          │    ✅    │          │ QS       │
│ ├─ Consistency modes   │          │    ✅    │          │ QS       │
│ ├─ Replication lag     │          │  tracks  │          │ QS       │
│ └─ Config management   │  stores  │          │  ✅→Core │ CP→Core  │
└────────────────────────┴──────────┴──────────┴──────────┴──────────┘

Legend: ✅ = owns/implements, proxy = passes through, →Core = delegates to Core
        ⚠️ = gap or overlap needing resolution
```

---

## Critical Gaps Identified

### Gap 1: Tenant Billing/Subscription Store (MEDIUM — downgraded from HIGH)

```
  NOTE: As of v0.10.0+, tenant metadata is WAL-durable via event-sourced
  system streams. Tenant data survives Core restarts (SystemBootstrap replays).
  This gap was HIGH when tenants were in-memory only — now downgraded.

  REMAINING GAP: No persistent store for billing/subscription data
  (LemonSqueezy IDs, usage counters, overage tracking).
  This data has no natural home in Core's event-sourced system.
```

### Gap 2: Auth Fragmentation (MEDIUM)

```
  Core:     /api/v1/auth/register + /login → issues token (HS256)
  QS:      OAuth → JOSE JWT (HS256, 7d TTL) → stateless (no DB)
  CP:      validates JWT with own secret → RBAC roles

  No shared identity provider. No token exchange.
  A QS JWT won't authenticate to CP and vice versa.
```

### Gap 3: Operation History Volatility (LOW)

```
  CP tracks operations (snapshot, compaction, replay) in MemoryOperationRepository.
  On CP restart → all operation history is lost.
  Should delegate to Core like tenant/audit/config already do.
```

### Gap 4: Missing CP <-> QS Communication (MEDIUM)

```
  Current interactions:
    QS ──► CP: cluster_health() only
    CP ──► QS: nothing

  Missing:
    CP tenant suspend → QS should block requests for that tenant
    CP quota update → QS should update enforcement limits
    CP policy change → QS should apply new policies
    QS usage data → CP should see billing/usage metrics
```

---

## Recommended Target Architecture (C4 Level 2)

```
┌─────────────────────────────────────────────────────────────────────┐
│                     AllSource DBaaS Platform                        │
│                                                                     │
│  ┌────────────────────┐         ┌────────────────────┐             │
│  │   Query Service    │         │   Control Plane    │             │
│  │   (Data Plane)     │         │   (Mgmt Plane)     │             │
│  │                    │         │                    │             │
│  │ • Event API proxy  │         │ • Tenant lifecycle │             │
│  │ • Consistency route│◄───────►│ • RBAC + Policies  │             │
│  │ • Read-your-writes │  events │ • Operations mgmt  │             │
│  │ • Analytics        │  or     │ • Audit trail      │             │
│  │ • Rate limiting    │  shared │ • Config mgmt      │             │
│  │                    │  auth   │ • Cluster overview  │             │
│  │ REMOVES:           │         │                    │             │
│  │ ✗ own tenant store │         │ KEEPS:             │             │
│  │ ✗ own user store   │         │ • Policy engine    │             │
│  │                    │         │ • Ops scheduling   │             │
│  │ KEEPS:             │         │                    │             │
│  │ • Billing (Lemon)  │         │ ADDS:              │             │
│  │ • Usage metering   │         │ • Notifies QS on   │             │
│  │ • Quota enforce    │         │   tenant changes   │             │
│  └─────────┬──────────┘         └─────────┬──────────┘             │
│            │                              │                        │
│            └──────────┬───────────────────┘                        │
│                       ▼                                            │
│            ┌──────────────────────┐                                │
│            │    AllSource Core    │                                │
│            │  (Single Source of   │                                │
│            │   Truth for ALL)     │                                │
│            │                     │                                │
│            │ Events + Tenants +  │                                │
│            │ Users + Auth +      │                                │
│            │ Audit + Config      │                                │
│            └──────────────────────┘                                │
│                                                                     │
│  PostgreSQL ──► billing/subscription metadata ONLY                 │
│                 (LemonSqueezy IDs, overage tracking)               │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Summary of Gaps by Priority

| # | Gap | Severity | Impact |
|---|-----|----------|--------|
| 1 | **No billing/subscription store** — tenant metadata is now WAL-durable, but billing data (LemonSqueezy IDs, usage) has no persistent home | MEDIUM | No billing persistence; tenant metadata itself is durable since v0.10.0 |
| 2 | **Auth fragmentation** — 3 separate JWT/auth systems | MEDIUM | No SSO; tokens aren't portable across services |
| 3 | **No CP->QS eventing** — CP state changes don't propagate | MEDIUM | Quota changes, suspensions, policy updates invisible to QS |
| 4 | **Operation history in-memory** — CP MemoryOperationRepo | LOW | Op history lost on restart; inconsistent with audit/config being Core-backed |
| 5 | **Backup is a stub** — CP exposes `/backup` route but no implementation | LOW | Admin API promises backup but doesn't deliver |

As of v0.10.0+, **no service uses PostgreSQL**. Query Service is fully stateless, Control Plane delegates to Core, and Core stores tenant metadata via event-sourced system streams (WAL-durable). Tenant metadata survives restarts. The remaining gap is billing/subscription data (LemonSqueezy IDs, usage counters) which has no persistent store. The next architectural milestone should add PostgreSQL for billing metadata only, with Core remaining the source of truth for event and tenant data. See `docs/current/TENANT_ARCHITECTURE.md` and ADR-005.
