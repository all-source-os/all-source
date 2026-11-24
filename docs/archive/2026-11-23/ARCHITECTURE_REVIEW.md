# AllSource Architecture Review

> **Note (2026-03-01)**: This review was written when the Query Service used PostgreSQL for user/tenant storage. As of v0.10.0, **Query Service is fully stateless** (no PostgreSQL). Tenant metadata is now WAL-durable via Core's event-sourced system streams (see ADR-005 and `docs/current/TENANT_ARCHITECTURE.md`). The "ideal state" recommendations in Sections 4-6 (Better Auth, JWKS, unified tenant authority) remain valid goals, but the "current state" sections (1-3) show a PostgreSQL dependency that no longer exists. The `SERVICE_RESPONSIBILITY_REALIGNMENT.md` proposal is partially superseded — its premise that QS uses PostgreSQL is outdated.

## Executive Summary

AllSource is a polyglot event sourcing platform with 5 services. This review identifies **3 disconnected auth stores**, **3 disconnected tenant stores**, and **broken email/password auth** as the primary architectural issues. The data flow for events is well-designed, but the identity and tenant management layers need consolidation.

---

## 1. Current State — System Context (C4 Level 1)

```mermaid
C4Context
    title System Context — Current State

    Person(user, "Platform User", "Developer using AllSource<br/>to store/query events")
    Person(aiAgent, "AI Agent", "Claude Desktop via MCP")

    System_Boundary(allsource, "AllSource Platform") {
        System(web, "Web Dashboard", "Next.js · Port 3000<br/>UI + cookie proxy")
        System(queryService, "Query Service", "Elixir/Phoenix · Port 3902<br/>API gateway + auth + billing")
        System(core, "Core Engine", "Rust/Axum · Port 3900<br/>Event store + processing")
        System(controlPlane, "Control Plane", "Go/Gin · Port 3901<br/>Cluster ops (mostly unused)")
        System(mcpServer, "MCP Server", "Elixir · Port 4000<br/>AI agent interface")
    }

    System_Ext(google, "Google OAuth")
    System_Ext(github, "GitHub OAuth")
    System_Ext(lemonSqueezy, "LemonSqueezy", "Billing provider")
    System_Ext(postgres, "PostgreSQL", "Persistent storage")

    Rel(user, web, "Uses", "HTTPS")
    Rel(aiAgent, mcpServer, "Uses", "JSON-RPC/stdio")
    Rel(web, queryService, "Proxies API calls", "HTTP + cookies")
    Rel(queryService, core, "Forwards events/queries", "HTTP + WebSocket")
    Rel(queryService, postgres, "Stores users, tenants, keys", "Ecto/TCP")
    Rel(queryService, google, "OAuth flow", "HTTPS")
    Rel(queryService, github, "OAuth flow", "HTTPS")
    Rel(queryService, lemonSqueezy, "Billing webhooks", "HTTPS")
    Rel(controlPlane, core, "Proxies (unused)", "HTTP")
    Rel(mcpServer, core, "Queries events", "HTTP")
```

---

## 2. Current State — Container Diagram (C4 Level 2)

### 2a. Authentication & Identity Flow

```mermaid
flowchart TB
    subgraph Browser
        LoginPage["Login Page<br/><i>email/password forms + OAuth buttons</i>"]
    end

    subgraph Web["Web Dashboard (Next.js :3000)"]
        Middleware["middleware.ts<br/><i>Checks auth_token cookie</i>"]
        CallbackRoute["/api/auth/callback<br/><i>Sets httpOnly cookie</i>"]
        SessionRoute["/api/auth/session<br/><i>Proxies to Query Service</i>"]
        AuthStore["Zustand auth store<br/><i>localStorage persistence</i>"]
    end

    subgraph QS["Query Service (Elixir :3902)"]
        Ueberauth["Ueberauth<br/><i>OAuth redirect/callback</i>"]
        Guardian["Guardian<br/><i>JWT HS512, 1hr TTL</i>"]
        AuthPipeline["AuthPipeline plug<br/><i>VerifyHeader → EnsureAuth → LoadResource</i>"]
        QSUsers[("users table<br/><i>email, provider, google_id,<br/>github_id, tenant_id</i>")]
    end

    subgraph Core["Core Engine (Rust :3900)"]
        AuthManager["AuthManager<br/><i>JWT HS256, 24hr TTL</i>"]
        CoreUsers[("DashMap&lt;Uuid, User&gt;<br/><i>In-memory only!<br/>username, password_hash,<br/>role, tenant_id</i>")]
    end

    subgraph CP["Control Plane (Go :3901)"]
        CPAuth["AuthClient<br/><i>JWT HS256</i>"]
        CPUsers[("MemoryUserRepo<br/><i>In-memory only!<br/>username, role, tenant_id</i>")]
    end

    LoginPage -->|"OAuth: redirect to /api/auth/google"| Ueberauth
    LoginPage -->|"Email: POST /api/auth/login"| QS
    LoginPage -.->|"❌ NOT IMPLEMENTED<br/>on backend"| QS

    Ueberauth -->|"callback with token"| CallbackRoute
    CallbackRoute -->|"sets auth_token cookie"| Browser
    Middleware -->|"reads auth_token cookie"| Middleware
    SessionRoute -->|"GET /api/auth/me"| AuthPipeline

    Guardian --> QSUsers
    AuthManager --> CoreUsers
    CPAuth --> CPUsers

    style CoreUsers fill:#ff6b6b,color:#fff
    style CPUsers fill:#ff6b6b,color:#fff
    style LoginPage fill:#ffa94d,color:#000

    classDef broken stroke:#ff0000,stroke-width:3px,stroke-dasharray: 5 5
    class LoginPage broken
```

**Key Problems:**
- **3 separate user/tenant stores** that never sync (Core and Control Plane user/tenant metadata is in-memory, lost on restart — note: this refers to user/tenant data only, NOT event storage which is durable via WAL + Parquet)
- **Email/password login is broken** — frontend forms exist, backend only handles OAuth
- Core and Control Plane each maintain their own JWT signing with different secrets
- No cross-service token validation possible

### 2b. Tenant Management Flow

```mermaid
flowchart TB
    subgraph Web["Web Dashboard (Next.js :3000)"]
        TenantUI["Sidebar + Settings<br/><i>Displays tenant name, tier, usage</i>"]
        BillingUI["Billing Page<br/><i>Checkout, portal, overage</i>"]
    end

    subgraph QS["Query Service (Elixir :3902)"]
        TenantCtx["TenantContext plug<br/><i>Validates subscription status</i>"]
        UsageEnf["UsageEnforcement plug<br/><i>Checks events/queries quota</i>"]
        RateLimit["RateLimiter<br/><i>ETS-based token bucket per tenant</i>"]
        QSTenant[("tenants table (PostgreSQL)<br/><i>name, slug, subscription_tier,<br/>events_quota, queries_quota,<br/>events_used, queries_used,<br/>overage_enabled, lemon_squeezy_*</i>")]
        QSUsage[("usage_records table<br/><i>Audit trail of consumption</i>")]
    end

    subgraph Core["Core Engine (Rust :3900)"]
        CoreTenantMW["tenant_isolation_middleware<br/><i>Validates tenant exists + active</i>"]
        CoreTenant[("InMemoryTenantRepo<br/><i>DashMap&lt;TenantId, Tenant&gt;<br/>In-memory only!</i>")]
        CoreRateLimit["RateLimiter<br/><i>Per-tenant rate limiting</i>"]
    end

    subgraph CP["Control Plane (Go :3901)"]
        CPTenant[("Tenant entity<br/><i>In-memory only!</i>")]
    end

    subgraph External
        LS["LemonSqueezy<br/><i>Subscription billing</i>"]
    end

    TenantUI -->|"GET /api/tenant"| QSTenant
    BillingUI -->|"POST /api/billing/checkout"| LS
    LS -->|"webhook"| QSTenant
    TenantCtx --> QSTenant
    UsageEnf --> QSTenant
    UsageEnf --> QSUsage
    CoreTenantMW --> CoreTenant

    style CoreTenant fill:#ff6b6b,color:#fff
    style CPTenant fill:#ff6b6b,color:#fff
    style QSTenant fill:#51cf66,color:#000
```

**Key Problems:**
- **Query Service PostgreSQL** is the only persistent user/tenant store (green) — source of truth for billing and auth
- **Core has its own in-memory tenant store** (red) — not synced with Query Service
- **Control Plane has its own tenant entity** (red) — not synced
- When Query Service forwards events to Core, tenant_id is passed as a parameter but Core doesn't validate it against Query Service's tenant store

### 2c. Event Data Flow

```mermaid
flowchart LR
    subgraph Clients
        WebUI["Web Dashboard<br/>:3000"]
        SDK["SDK / API Key<br/><i>Direct HTTP</i>"]
        MCP["MCP Server<br/>:4000"]
    end

    subgraph QS["Query Service :3902"]
        direction TB
        Auth["AuthPipeline"]
        Tenant["TenantContext"]
        Quota["UsageEnforcement"]
        Rate["RateLimiter"]
        EventCtrl["EventController"]
        QueryCtrl["QueryController"]
        ProjCtrl["ProjectionController"]
        RustClient["RustCoreClient<br/><i>Tesla HTTP + retry</i>"]
    end

    subgraph Core["Core Engine :3900"]
        direction TB
        CoreAuth["auth_middleware"]
        Ingest["Event Ingestion<br/><i>Schema validation → WAL → Index</i>"]
        Query["Query Engine<br/><i>Time-travel, filtering</i>"]
        Stream["Stream Processing<br/><i>6 operators</i>"]
        Storage[("Parquet + WAL<br/><i>Columnar storage</i>")]
        WS["WebSocket<br/><i>Real-time broadcast</i>"]
    end

    subgraph PG["PostgreSQL :5432"]
        Users[("users")]
        Tenants[("tenants")]
        UsageRec[("usage_records")]
        ApiKeys[("api_keys")]
    end

    WebUI -->|"HTTP + cookie"| Auth
    SDK -->|"HTTP + API key"| Auth
    MCP -->|"HTTP direct"| Core

    Auth --> Tenant --> Quota --> Rate
    Rate --> EventCtrl & QueryCtrl & ProjCtrl
    EventCtrl & QueryCtrl --> RustClient
    RustClient -->|"HTTP with tenant_id"| CoreAuth
    CoreAuth --> Ingest & Query
    Ingest --> Storage
    Query --> Storage
    Core --> WS -->|"ws:// stream"| QS

    Quota -.->|"record usage"| UsageRec
    Tenant -.->|"validate subscription"| Tenants
    Auth -.->|"validate JWT"| Users
```

**This flow is well-designed.** The pipeline is: Auth → Tenant → Quota → Rate Limit → Controller → Core. The concern is the double-auth: Query Service validates its JWT, then Core has its own auth middleware (bypassed in dev mode).

---

## 3. Service Responsibility Matrix

| Responsibility | Web (Next.js) | Query Service (Elixir) | Core (Rust) | Control Plane (Go) | MCP Server (Elixir) |
|---|---|---|---|---|---|
| **User Authentication** | Cookie proxy | Ueberauth + Guardian (OAuth JWT) | AuthManager (username/password JWT) | AuthClient (JWT) | None |
| **User Store** | None (Zustand cache) | PostgreSQL (persistent) | DashMap (in-memory) | Map (in-memory) | None |
| **Tenant Store** | None (Zustand cache) | PostgreSQL (persistent) | DashMap (in-memory) | Struct (in-memory) | None |
| **Tenant Billing** | UI only | LemonSqueezy integration | None | None | None |
| **Usage Metering** | UI only | PostgreSQL + atomic counters | None | None | None |
| **Rate Limiting** | None | ETS token bucket | Per-tenant rate limiter | None | None |
| **Event Storage** | None | Proxy to Core | Parquet + WAL (primary) | None | None |
| **Event Querying** | UI only | DSL + proxy to Core | Arrow columnar engine | None | MCP tools |
| **Projections** | UI only | GenServer-based | Pipeline processing | None | None |
| **Schema Registry** | None | Proxy to Core | JSON Schema validation | None | None |
| **API Key Management** | UI only | PostgreSQL + SHA-256 | DashMap (in-memory) | None | None |
| **RBAC** | None | Subscription tier | Role-based (4 roles) | Role-based (4 roles) | None |
| **Observability** | None | Phoenix telemetry | /metrics endpoint | OTLP + audit log | None |

### Duplicated Responsibilities (Issues)

| Duplicated Concern | Services | Problem |
|---|---|---|
| **Auth/Identity** | Query Service, Core, Control Plane | 3 different JWT implementations, 3 user stores, no sync |
| **Tenant Management** | Query Service, Core, Control Plane | 3 tenant stores, only Query Service is persistent |
| **API Key Management** | Query Service, Core | 2 independent key stores, different formats |
| **Rate Limiting** | Query Service, Core | 2 independent rate limiters, not coordinated |
| **RBAC** | Core, Control Plane | Both define identical 4-role system independently |

---

## 4. Ideal State — System Context (C4 Level 1)

```mermaid
C4Context
    title System Context — Ideal State

    Person(user, "Platform User", "Developer using AllSource")
    Person(aiAgent, "AI Agent", "Claude Desktop via MCP")

    System_Boundary(allsource, "AllSource Platform") {
        System(web, "Web App + Auth", "Next.js · Port 3000<br/>UI + Better Auth (identity provider)")
        System(queryService, "Query Service", "Elixir/Phoenix · Port 3902<br/>API gateway + billing + projections")
        System(core, "Core Engine", "Rust/Axum · Port 3900<br/>Event store + processing<br/>(no auth, trusts upstream)")
        System(mcpServer, "MCP Server", "Elixir · Port 4000<br/>AI agent interface")
    }

    System_Ext(google, "Google OAuth")
    System_Ext(github, "GitHub OAuth")
    System_Ext(lemonSqueezy, "LemonSqueezy")
    System_Ext(postgres, "PostgreSQL")

    Rel(user, web, "Uses", "HTTPS")
    Rel(aiAgent, mcpServer, "Uses", "JSON-RPC/stdio")
    Rel(web, queryService, "API calls + JWT", "HTTP")
    Rel(queryService, core, "Forwards events/queries", "HTTP (internal, trusted)")
    Rel(queryService, postgres, "Users, tenants, keys, usage", "TCP")
    Rel(web, postgres, "Better Auth tables", "TCP")
    Rel(web, google, "OAuth", "HTTPS")
    Rel(web, github, "OAuth", "HTTPS")
    Rel(queryService, lemonSqueezy, "Billing", "HTTPS")
    Rel(mcpServer, queryService, "Authenticated API calls", "HTTP + JWT")
```

**Key changes from current state:**
1. **Control Plane removed** — its responsibilities folded into Query Service and Core
2. **Web App becomes the identity provider** (Better Auth or similar)
3. **Core drops auth** — trusts internal network, Query Service handles access control
4. **MCP Server routes through Query Service** instead of hitting Core directly (gets tenant isolation)

---

## 5. Ideal State — Container Diagram (C4 Level 2)

### 5a. Unified Authentication Flow

```mermaid
flowchart TB
    subgraph Browser
        LoginPage["Login Page<br/><i>Email/password + OAuth</i>"]
    end

    subgraph Web["Web App (Next.js :3000) — Identity Provider"]
        BetterAuth["Better Auth Server<br/><i>Email/password, OAuth, sessions</i>"]
        JWKS["/.well-known/jwks.json<br/><i>Public keys for JWT verification</i>"]
        AuthClient["Better Auth Client<br/><i>signIn.email(), signIn.social()</i>"]
        CatchAll["/api/auth/[...all]<br/><i>Catch-all route handler</i>"]
    end

    subgraph PG["PostgreSQL"]
        BAUser[("ba_user<br/><i>Better Auth user table</i>")]
        BASession[("ba_session<br/><i>Better Auth sessions</i>")]
        BAAccount[("ba_account<br/><i>OAuth provider links</i>")]
    end

    subgraph QS["Query Service (Elixir :3902)"]
        JWKSValidator["JWKS JWT Validator<br/><i>Fetches public keys from Web App<br/>Caches in ETS, refreshes hourly</i>"]
        TenantCtx["TenantContext plug<br/><i>find_or_create user + tenant</i>"]
        QSTenant[("tenants table<br/><i>Single source of truth</i>")]
    end

    subgraph Core["Core Engine (Rust :3900)"]
        InternalOnly["Internal API<br/><i>No auth middleware<br/>Trusts Query Service</i>"]
    end

    LoginPage -->|"authClient.signIn.email()"| CatchAll
    LoginPage -->|"authClient.signIn.social({provider: 'google'})"| CatchAll
    CatchAll --> BetterAuth
    BetterAuth --> BAUser & BASession & BAAccount

    AuthClient -->|"JWT in Authorization header"| JWKSValidator
    JWKSValidator -->|"fetch public keys"| JWKS
    JWKSValidator --> TenantCtx
    TenantCtx --> QSTenant
    TenantCtx -->|"trusted internal call"| InternalOnly

    style BetterAuth fill:#51cf66,color:#000
    style JWKS fill:#51cf66,color:#000
    style JWKSValidator fill:#51cf66,color:#000
    style InternalOnly fill:#74c0fc,color:#000
```

**Benefits:**
- **Single identity provider** (Better Auth in Web App)
- **JWKS-based validation** — no shared secrets between services
- **Email/password works** — Better Auth handles it natively
- **OAuth consolidated** — Better Auth handles Google/GitHub
- **Core simplified** — no auth complexity, trusts internal network

### 5b. Unified Tenant Management Flow

```mermaid
flowchart TB
    subgraph Web["Web App :3000"]
        UI["Dashboard UI<br/><i>Tenant settings, billing, usage</i>"]
    end

    subgraph QS["Query Service :3902 — Tenant Authority"]
        TenantCtx["TenantContext plug"]
        TenantSvc["Tenant Service<br/><i>CRUD + subscription management</i>"]
        UsageSvc["Usage Service<br/><i>Metering + quota enforcement</i>"]
        BillingSvc["Billing Service<br/><i>LemonSqueezy integration</i>"]
        RateLimit["Rate Limiter<br/><i>Tier-based per tenant</i>"]
    end

    subgraph PG["PostgreSQL — Single Source of Truth"]
        Tenants[("tenants<br/><i>Subscription, quotas, billing</i>")]
        Users[("users<br/><i>Mapped to Better Auth by email</i>")]
        Usage[("usage_records<br/><i>Consumption audit trail</i>")]
        ApiKeys[("api_keys<br/><i>Tenant-scoped, SHA-256</i>")]
    end

    subgraph Core["Core Engine :3900"]
        CoreAPI["Event API<br/><i>Receives tenant_id from<br/>Query Service in headers</i>"]
    end

    subgraph LS["LemonSqueezy"]
        Webhooks["Subscription Webhooks"]
    end

    UI -->|"GET/PUT /api/tenant"| TenantSvc
    UI -->|"POST /api/billing/checkout"| BillingSvc
    Webhooks -->|"POST /api/webhooks/lemonsqueezy"| BillingSvc

    TenantCtx --> Tenants
    TenantSvc --> Tenants
    UsageSvc --> Usage & Tenants
    BillingSvc --> Tenants & LS
    RateLimit -->|"tier from tenant"| Tenants

    TenantCtx -->|"X-Tenant-ID header"| CoreAPI

    style Tenants fill:#51cf66,color:#000
    style Users fill:#51cf66,color:#000
```

**Benefits:**
- **Single tenant authority** — Query Service + PostgreSQL
- **Core receives tenant_id** as a trusted header, doesn't store tenants
- **No tenant duplication** across services
- **Billing, quotas, usage** all in one place

### 5c. Ideal Event Data Flow

```mermaid
flowchart LR
    subgraph Clients
        WebUI["Web Dashboard"]
        SDK["SDK / API Key"]
        MCP["MCP Server"]
    end

    subgraph QS["Query Service :3902<br/>(Gateway + Tenant Authority)"]
        direction TB
        JWKS["JWKS Validator"]
        APIKeyAuth["API Key Auth"]
        Tenant["TenantContext"]
        Quota["UsageEnforcement"]
        Rate["RateLimiter"]
        Controllers["Event/Query Controllers"]
        RustClient["RustCoreClient"]
    end

    subgraph Core["Core Engine :3900<br/>(Internal, No Auth)"]
        direction TB
        Ingest["Event Ingestion"]
        Query["Query Engine"]
        Storage[("Parquet + WAL")]
        WS["WebSocket Stream"]
    end

    subgraph PG["PostgreSQL"]
        TenantDB[("tenants + usage")]
    end

    WebUI -->|"JWT (Better Auth)"| JWKS
    SDK -->|"API Key"| APIKeyAuth
    MCP -->|"JWT or API Key"| QS

    JWKS & APIKeyAuth --> Tenant
    Tenant --> Quota --> Rate --> Controllers
    Controllers --> RustClient
    RustClient -->|"HTTP internal<br/>X-Tenant-ID header"| Ingest & Query
    Ingest & Query --> Storage
    Core --> WS -->|"ws://"| QS

    Tenant -.-> TenantDB
    Quota -.-> TenantDB
```

**Key improvement:** Single entry point (Query Service) for all clients. Core is an internal service only.

---

## 6. Migration Path: Current → Ideal

### Phase 1: Consolidate Auth (High Priority)
1. Add Better Auth to Web App (email/password + OAuth + JWKS)
2. Add JWKS JWT validation to Query Service (replace Guardian)
3. Keep Guardian as fallback during migration
4. **Outcome:** Working email/password login, single identity provider

### Phase 2: Simplify Core Auth (Medium Priority)
1. Remove AuthManager from Core (users, JWT, API keys)
2. Core trusts internal network — only accepts requests from Query Service
3. Add network-level isolation (Docker network, no public exposure)
4. **Outcome:** Core is simpler, no auth duplication

### Phase 3: Remove Control Plane (Medium Priority)
1. Move audit logging to Query Service (already has usage_records)
2. Move cluster status endpoint to Core's existing /health
3. Delete Control Plane service entirely
4. **Outcome:** One fewer service to maintain, no orphaned auth store

### Phase 4: Route MCP Through Query Service (Low Priority)
1. MCP Server calls Query Service instead of Core directly
2. Gets tenant isolation, rate limiting, usage metering for free
3. **Outcome:** AI agents respect same quotas as human users

---

## 7. Risk Assessment

| Risk | Severity | Current Impact | Mitigation |
|---|---|---|---|
| Auth store divergence | **High** | Users can't log in with email/password | Phase 1: Better Auth |
| In-memory user/tenant metadata | **High** | Core/CP restart = lost user/tenant metadata (event data is durable via WAL) | Phase 2: Remove in-memory user/tenant stores |
| No cross-service auth | **Medium** | Can't validate tokens across services | Phase 1: JWKS |
| Tenant isolation gap | **Medium** | MCP bypasses quotas hitting Core directly | Phase 4: Route through QS |
| Control Plane rot | **Low** | Unused code accumulates tech debt | Phase 3: Remove |

---

## 8. Current vs Ideal — Side by Side

| Dimension | Current | Ideal |
|---|---|---|
| **Identity Provider** | 3 independent stores | 1 (Better Auth in Web App) |
| **Auth Protocol** | 3 different JWT implementations | JWKS public key verification |
| **Email/Password** | Broken (forms exist, no backend) | Working via Better Auth |
| **OAuth** | Query Service only (Ueberauth) | Better Auth (unified) |
| **Tenant Authority** | 3 stores, only QS persistent | 1 (Query Service PostgreSQL) |
| **API Entry Points** | 3 (Web→QS, SDK→QS, MCP→Core) | 1 (all through Query Service) |
| **Core Auth** | Full AuthManager + RBAC | None (internal service) |
| **Control Plane** | Exists but mostly unused | Removed |
| **Services Count** | 5 | 4 |
| **JWT Secrets** | 3 separate secrets | 0 secrets (JWKS asymmetric) |
