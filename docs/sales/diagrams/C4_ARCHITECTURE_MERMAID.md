# AllSource C4 Architecture Diagrams (Mermaid)

> Core 0.30.0. Current architecture: Core stores events in WAL + Parquet, with DashMap in-memory maps. No PostgreSQL or Redis store; Core's optional RESP3 listener (`ALLSOURCE_RESP_PORT`) speaks the Redis wire protocol only. Query Service is stateless.

These diagrams follow the C4 model for visualizing software architecture.
Render in any Mermaid-compatible tool (GitHub, Notion, VS Code, etc.)

---

## Level 1: System Context Diagram

Shows AllSource in the context of its users and external systems.

```mermaid
C4Context
    title AllSource System Context Diagram

    Person(developer, "Developer", "Builds applications using event sourcing patterns")
    Person(dataEngineer, "Data Engineer", "Manages data pipelines and analytics")
    Person(aiEngineer, "AI/ML Engineer", "Builds AI agents and LLM applications")
    Person(operator, "Platform Operator", "Manages infrastructure and security")

    System(allsource, "AllSource Platform", "AI-native event sourcing platform for temporal data intelligence. 469K events/sec, 11.9μs p99 query latency.")

    System_Ext(clientApps, "Client Applications", "Web, mobile, backend services consuming events")
    System_Ext(llmAgents, "LLM/AI Agents", "Claude, GPT, custom agents via MCP protocol")
    System_Ext(monitoring, "Monitoring Stack", "Prometheus, Grafana, Jaeger")
    System_Ext(authProvider, "Identity Provider", "OAuth providers (Google, GitHub)")

    Rel(developer, allsource, "Ingests events, queries history")
    Rel(dataEngineer, allsource, "Creates projections, exports data")
    Rel(aiEngineer, allsource, "Uses MCP tools for AI workflows")
    Rel(operator, allsource, "Configures tenants, monitors health")

    Rel(allsource, clientApps, "Serves events via REST/WebSocket")
    Rel(llmAgents, allsource, "Natural language queries via MCP")
    Rel(allsource, monitoring, "Exports metrics and traces")
    Rel(allsource, authProvider, "Validates tokens")

    UpdateLayoutConfig($c4ShapeInRow="3", $c4BoundaryInRow="1")
```

---

## Level 2: Container Diagram

Shows the high-level containers (services) that make up AllSource.

```mermaid
C4Container
    title AllSource Container Diagram

    Person(user, "User", "Developer, Data Engineer, or AI Engineer")
    Person(llm, "AI Agent", "Claude or other LLM via MCP")

    System_Boundary(allsource, "AllSource Platform") {
        Container(web, "Web Dashboard", "Next.js 16, React 19", "Real-time event visualization, management UI, OAuth login, billing UI")
        Container(controlPlane, "Control Plane", "Go 1.26.11, Gin v1.12.0", "Authentication, RBAC, audit logging, billing webhooks")
        Container(authService, "Auth Service", "Rust, better-auth", "OAuth, sessions, user management (event-sourced)")
        Container(core, "Event Store Core", "Rust 1.92, Axum", "High-performance event storage, indexing, schemas, projections")
        Container(queryService, "Query Service", "Elixir 1.17, Phoenix", "Advanced queries, real-time subscriptions, billing state")
        Container(mcpServer, "MCP Server", "Elixir, JSON-RPC", "73 MCP tool definitions for natural language interaction")

        ContainerDb(storage, "Event Storage", "Parquet + WAL", "Columnar storage with write-ahead log for durability")
    }

    System_Ext(lemonSqueezy, "LemonSqueezy", "Payment provider — subscriptions, invoices")
    System_Ext(authProvider, "OAuth Providers", "Google, GitHub")

    Rel(user, web, "Uses", "HTTPS")
    Rel(llm, mcpServer, "Queries via", "JSON-RPC 2.0/stdio")

    Rel(web, authService, "Auth flows", "HTTP/Internal")
    Rel(web, queryService, "API calls, billing status", "HTTP/Internal")
    Rel(web, controlPlane, "Checkout, portal", "HTTP/Internal")
    Rel(authService, core, "Writes auth events", "HTTP/Internal")
    Rel(authService, authProvider, "OAuth", "HTTPS")
    Rel(controlPlane, core, "Writes billing events", "HTTP/Internal")
    Rel(queryService, core, "Reads events", "HTTP/Internal")
    Rel(lemonSqueezy, controlPlane, "Webhooks", "HTTPS")
    Rel(controlPlane, lemonSqueezy, "Checkout sessions", "HTTPS")
    Rel(mcpServer, core, "Fetches events", "HTTP/Internal")
    Rel(mcpServer, queryService, "Executes queries", "HTTP/Internal")

    Rel(core, storage, "Reads/writes events")

    UpdateLayoutConfig($c4ShapeInRow="3", $c4BoundaryInRow="1")
```

---

## Level 3: Component Diagram - Event Store Core (Rust)

Detailed view of the Rust core service following Clean Architecture.

```mermaid
C4Component
    title AllSource Core - Component Diagram (Clean Architecture)

    Container_Boundary(core, "Event Store Core (Rust)") {

        Component_Ext(api, "REST API Layer", "Axum", "HTTP endpoints for event operations")

        Boundary(app, "Application Layer") {
            Component(ingestUC, "IngestEvent", "Use Case", "Validates and stores new events")
            Component(queryUC, "QueryEvents", "Use Case", "Retrieves events with filtering")
            Component(schemaUC, "ManageSchema", "Use Case", "Schema registry operations")
            Component(projectionUC, "ManageProjection", "Use Case", "Materialized view management")
            Component(tenantUC, "ManageTenant", "Use Case", "Multi-tenancy operations")
        }

        Boundary(domain, "Domain Layer") {
            Component(event, "Event Entity", "Domain", "Core event aggregate with validation")
            Component(stream, "EventStream", "Domain", "Ordered event sequence with versioning")
            Component(schema, "Schema Entity", "Domain", "JSON Schema validation rules")
            Component(projection, "Projection", "Domain", "Materialized view definitions")
            Component(tenant, "Tenant", "Domain", "Isolation and quota management")
        }

        Boundary(infra, "Infrastructure Layer") {
            Component(storage, "ParquetStorage", "Infrastructure", "Columnar file storage")
            Component(wal, "WriteAheadLog", "Infrastructure", "Durability guarantee via WAL")
            Component(index, "DashMapIndex", "Infrastructure", "Lock-free O(1) lookups")
            Component(auth, "JWTAuth", "Infrastructure", "Token validation and RBAC")
            Component(pipeline, "StreamPipeline", "Infrastructure", "6 operators: Filter, Map, etc.")
        }
    }

    Rel(api, ingestUC, "Calls")
    Rel(api, queryUC, "Calls")
    Rel(api, schemaUC, "Calls")
    Rel(api, tenantUC, "Calls")

    Rel(ingestUC, event, "Creates")
    Rel(ingestUC, schema, "Validates against")
    Rel(queryUC, stream, "Reads from")
    Rel(projectionUC, projection, "Manages")
    Rel(tenantUC, tenant, "Manages")

    Rel(ingestUC, storage, "Persists via")
    Rel(ingestUC, wal, "Logs to")
    Rel(ingestUC, index, "Updates")
    Rel(queryUC, index, "Queries")
    Rel(api, auth, "Authenticates via")

    UpdateLayoutConfig($c4ShapeInRow="4", $c4BoundaryInRow="1")
```

---

## Level 3: Component Diagram - Control Plane (Go)

```mermaid
C4Component
    title AllSource Control Plane - Component Diagram

    Container_Boundary(cp, "Control Plane (Go)") {

        Component(router, "Gin Router", "Framework", "HTTP request routing and middleware")

        Component(authHandler, "Auth Handler", "Handler", "Login, register, token refresh")
        Component(clusterHandler, "Cluster Handler", "Handler", "Health, status, metrics")
        Component(tenantHandler, "Tenant Handler", "Handler", "Tenant CRUD operations")
        Component(opsHandler, "Operations Handler", "Handler", "Backup, restore, replay")

        Component(jwtService, "JWT Service", "Service", "Token generation and validation")
        Component(rbacService, "RBAC Service", "Service", "4 roles, 7 permissions")
        Component(auditService, "Audit Service", "Service", "Operation logging")
        Component(traceService, "Tracing Service", "Service", "OpenTelemetry OTLP export")
        Component(proxyService, "Proxy Service", "Service", "Authenticated forwarding to Core")

        Component(policyEngine, "Policy Engine", "Service", "5 default + custom policies")
    }

    Rel(router, authHandler, "Routes /api/v1/auth/*")
    Rel(router, clusterHandler, "Routes /api/v1/status/services")
    Rel(router, tenantHandler, "Routes /api/v1/tenants/*")
    Rel(router, opsHandler, "Routes /api/v1/operations/*")

    Rel(authHandler, jwtService, "Uses")
    Rel(router, rbacService, "Checks permissions")
    Rel(router, auditService, "Logs operations")
    Rel(router, traceService, "Traces requests")
    Rel(router, policyEngine, "Enforces policies")
    Rel(clusterHandler, proxyService, "Forwards to Core")

    UpdateLayoutConfig($c4ShapeInRow="4", $c4BoundaryInRow="1")
```

---

## Level 3: Component Diagram - Query Service (Elixir)

```mermaid
C4Component
    title AllSource Query Service - Component Diagram

    Container_Boundary(qs, "Query Service (Elixir/Phoenix)") {

        Component(phoenix, "Phoenix Router", "Framework", "HTTP endpoints and channels")

        Component(eventCtrl, "Event Controller", "Controller", "CRUD and search operations")
        Component(queryCtrl, "Query Controller", "Controller", "DSL query execution")
        Component(projCtrl, "Projection Controller", "Controller", "Projection management")

        Component(queryDSL, "Query DSL", "Domain", "Fluent, pipe-friendly query building")
        Component(projServer, "Projection GenServer", "Application", "Real-time materialized views")
        Component(pipelineProc, "Pipeline Processor", "Application", "Event transformation chains")

        Component(coreClient, "Rust Core Client", "Infrastructure", "Tesla HTTP client")
        Component(wsChannel, "WebSocket Channel", "Infrastructure", "Real-time subscriptions")
        Component(broadwayProd, "Broadway Producer", "Infrastructure", "High-throughput processing")
    }

    Rel(phoenix, eventCtrl, "Routes /api/v1 event endpoints")
    Rel(phoenix, queryCtrl, "Routes /api/v1 query endpoints")
    Rel(phoenix, projCtrl, "Routes projection endpoints")

    Rel(queryCtrl, queryDSL, "Builds queries with")
    Rel(projCtrl, projServer, "Manages")
    Rel(eventCtrl, pipelineProc, "Transforms via")

    Rel(eventCtrl, coreClient, "Fetches from Core")
    Rel(projServer, coreClient, "Syncs with Core")
    Rel(phoenix, wsChannel, "Streams via")
    Rel(pipelineProc, broadwayProd, "Processes with")

    UpdateLayoutConfig($c4ShapeInRow="3", $c4BoundaryInRow="1")
```

---

## Level 3: Component Diagram - MCP Server

```mermaid
C4Component
    title AllSource MCP Server - Component Diagram

    Container_Boundary(mcp, "MCP Server (Elixir)") {

        Component(jsonrpc, "JSON-RPC Handler", "Protocol", "JSON-RPC 2.0 over stdio")

        Boundary(coreTools, "Core Query Tools") {
            Component(queryEvents, "query_events", "Tool", "Event retrieval with filters")
            Component(reconstruct, "reconstruct_state", "Tool", "Point-in-time entity state")
            Component(analyze, "analyze_changes", "Tool", "Change detection and diffs")
            Component(patterns, "find_patterns", "Tool", "Pattern recognition")
        }

        Boundary(searchTools, "Search Tools") {
            Component(semantic, "semantic_search_events", "Tool", "Semantic search over events")
            Component(hybrid, "hybrid_search", "Tool", "Combined text + semantic")
        }

        Boundary(mgmtTools, "Management Tools") {
            Component(deleteEvt, "delete_events", "Tool", "Delete events")
            Component(archiveEvt, "archive_events", "Tool", "Cold storage migration")
            Component(exportEvt, "export_events", "Tool", "JSON, JSONL, CSV, Parquet export")
            Component(importEvt, "import_events", "Tool", "Bulk import with dedup")
        }

        Component(toonFormat, "TOON Formatter", "Service", "About 50% fewer tokens than JSON for tabular data")
        Component(sessionMgr, "Session Manager", "Service", "Context and state tracking")
    }

    Rel(jsonrpc, queryEvents, "Dispatches")
    Rel(jsonrpc, reconstruct, "Dispatches")
    Rel(jsonrpc, semantic, "Dispatches")
    Rel(jsonrpc, deleteEvt, "Dispatches")

    Rel(queryEvents, toonFormat, "Formats response")
    Rel(jsonrpc, sessionMgr, "Tracks context")

    UpdateLayoutConfig($c4ShapeInRow="4", $c4BoundaryInRow="1")
```

---

## Data Flow Diagram: Event Ingestion

```mermaid
sequenceDiagram
    participant Client
    participant ControlPlane as Control Plane (Go)
    participant Core as Event Store (Rust)
    participant WAL as Write-Ahead Log
    participant Index as DashMap Index
    participant Storage as Parquet Storage
    participant WS as WebSocket

    Client->>ControlPlane: POST /api/v1/events
    ControlPlane->>ControlPlane: Validate JWT
    ControlPlane->>ControlPlane: Check RBAC
    ControlPlane->>ControlPlane: Log to Audit
    ControlPlane->>Core: Forward request

    Core->>Core: Validate JSON Schema
    Core->>WAL: Write to WAL (durability)
    Core->>Index: Update entity/type indexes
    Core->>Storage: Batch to Parquet
    Core->>WS: Broadcast to subscribers

    Core-->>ControlPlane: 200 OK + event_id
    ControlPlane-->>Client: Response
```

---

## Deployment Diagram

```mermaid
C4Deployment
    title AllSource Deployment - Fly.io

    Deployment_Node(fly, "Fly.io", "allsource organization") {

        Deployment_Node(frontends, "Frontends") {
            Container(webApp, "allsource-web", "Next.js", "all-source.xyz and www")
            Container(adminApp, "allsource-admin", "Next.js", "admin.all-source.xyz")
        }

        Deployment_Node(services, "Services") {
            Container(cpApp, "allsource-control-plane", "Go", "api.all-source.xyz: auth, onboarding, billing")
            Container(authApp, "allsource-auth", "Rust, better-auth", "OAuth, sessions")
            Container(qsApp, "allsource-query", "Elixir, Phoenix", "Query Service")
            Container(primeApp, "allsource-prime", "Rust", "Prime MCP server")
        }

        Deployment_Node(coreNode, "Event Store") {
            ContainerDb(coreApp, "allsource-core", "Rust", "Event store, volume allsource_data (WAL + Parquet)")
        }
    }

    Rel(webApp, authApp, "Auth flows")
    Rel(webApp, qsApp, "API calls, billing status")
    Rel(webApp, cpApp, "Checkout, portal")
    Rel(authApp, coreApp, "Writes auth events")
    Rel(cpApp, coreApp, "Writes billing events")
    Rel(qsApp, coreApp, "Reads events")
```

---

## Usage Notes

### Rendering These Diagrams

1. **GitHub**: Paste directly in markdown files - renders automatically
2. **VS Code**: Use "Markdown Preview Mermaid Support" extension
3. **Notion**: Paste in code block with "mermaid" language
4. **Mermaid Live**: https://mermaid.live for online editing
5. **Export**: Use mermaid-cli (`mmdc`) for PNG/SVG export

### Customization

- Adjust `UpdateLayoutConfig` for different layouts
- Modify colors via CSS classes or Mermaid themes
- Add/remove components as architecture evolves

### C4 Model Conventions

- **Level 1 (Context)**: System + external actors/systems
- **Level 2 (Container)**: Services, databases, applications
- **Level 3 (Component)**: Internal components of a container
- **Level 4 (Code)**: Class diagrams (not included here)
