/*
 * AllSource C4 Architecture - Structurizr DSL
 *
 * Use with:
 * - Structurizr Lite: https://structurizr.com/help/lite
 * - Structurizr CLI: structurizr-cli export -workspace allsource.dsl -format plantuml
 * - Online: https://structurizr.com/dsl
 */

workspace "AllSource" "AI-native event sourcing platform for temporal data intelligence" {

    !identifiers hierarchical

    model {
        # ==================== People ====================
        developer = person "Developer" "Builds applications using event sourcing patterns" "User"
        dataEngineer = person "Data Engineer" "Manages data pipelines, creates projections, exports data" "User"
        aiEngineer = person "AI/ML Engineer" "Builds AI agents and LLM applications using MCP" "User"
        operator = person "Platform Operator" "Manages infrastructure, security, and multi-tenancy" "User"

        # ==================== External Systems ====================
        clientApps = softwareSystem "Client Applications" "Web, mobile, and backend services that consume events" "External"
        llmAgents = softwareSystem "LLM/AI Agents" "Claude, GPT, and custom AI agents interacting via MCP protocol" "External"
        monitoringStack = softwareSystem "Monitoring Stack" "Prometheus, Grafana, Jaeger for observability" "External"
        identityProvider = softwareSystem "Identity Provider" "OAuth providers (Google, GitHub)" "External"

        # ==================== AllSource System ====================
        allsource = softwareSystem "AllSource Platform" "AI-native event sourcing platform. 469K events/sec, 11.9us p99 latency, 73 MCP tool definitions." {

            # --- Web Dashboard ---
            webDashboard = container "Web Dashboard" "Real-time event visualization, management UI, OAuth login" "Next.js 16, React 19, Tailwind CSS" "Web Browser"

            # --- Control Plane ---
            controlPlane = container "Control Plane" "Authentication, authorization, audit logging, request routing" "Go 1.26.11, Gin v1.12.0" "Service" {
                ginRouter = component "Gin Router" "HTTP request routing and middleware chain" "Gin"
                authHandler = component "Auth Handler" "Login, register, token refresh, user management" "Handler"
                clusterHandler = component "Cluster Handler" "Health checks, cluster status, metrics export" "Handler"
                tenantHandler = component "Tenant Handler" "Multi-tenant CRUD operations" "Handler"
                operationsHandler = component "Operations Handler" "Backup, restore, snapshot, replay operations" "Handler"
                jwtService = component "JWT Service" "Token generation, validation, and refresh" "Service"
                rbacService = component "RBAC Service" "Role-based access control with 4 roles, 7 permissions" "Service"
                auditService = component "Audit Service" "Comprehensive operation logging in JSON format" "Service"
                tracingService = component "Tracing Service" "OpenTelemetry OTLP export for distributed tracing" "Service"
                proxyService = component "Proxy Service" "Authenticated request forwarding to backend services" "Service"
                policyEngine = component "Policy Engine" "5 default security policies plus custom policy support" "Service"
            }

            # --- Event Store Core ---
            eventStoreCore = container "Event Store Core" "High-performance event storage with Clean Architecture" "Rust 1.92, Axum, Tokio" "Service" {
                restApi = component "REST API" "HTTP endpoints for all event operations" "Axum"

                # Application Layer
                ingestEventUC = component "IngestEvent Use Case" "Validates schema, writes to WAL, updates indexes" "Use Case"
                queryEventsUC = component "QueryEvents Use Case" "Retrieves events with filtering, pagination, time-travel" "Use Case"
                manageSchemaUC = component "ManageSchema Use Case" "Schema registry with versioning and compatibility" "Use Case"
                manageProjectionUC = component "ManageProjection Use Case" "Materialized view creation and updates" "Use Case"
                manageTenantUC = component "ManageTenant Use Case" "Tenant isolation, quotas, and configuration" "Use Case"

                # Domain Layer
                eventEntity = component "Event Entity" "Core event aggregate with validation invariants" "Entity"
                eventStream = component "EventStream" "Ordered event sequence with version tracking" "Aggregate"
                schemaEntity = component "Schema Entity" "JSON Schema definitions with versioning" "Entity"
                projectionEntity = component "Projection Entity" "Materialized view definitions" "Entity"
                tenantEntity = component "Tenant Entity" "Multi-tenancy configuration and quotas" "Entity"

                # Infrastructure Layer
                parquetStorage = component "Parquet Storage" "Columnar storage" "Infrastructure"
                writeAheadLog = component "Write-Ahead Log" "Durability guarantee with segment rotation" "Infrastructure"
                dashMapIndex = component "DashMap Index" "Lock-free O(1) entity and type lookups" "Infrastructure"
                jwtAuth = component "JWT Auth" "Token validation and RBAC enforcement" "Infrastructure"
                streamPipeline = component "Stream Pipeline" "6 operators: Filter, Map, Reduce, Window, Enrich, Branch" "Infrastructure"
            }

            # --- Query Service ---
            queryService = container "Query Service" "Advanced queries, real-time subscriptions, pipeline processing" "Elixir 1.17, Phoenix, Broadway" "Service" {
                phoenixRouter = component "Phoenix Router" "HTTP endpoints and WebSocket channels" "Phoenix"
                eventController = component "Event Controller" "Event CRUD and search operations" "Controller"
                queryController = component "Query Controller" "DSL query execution and optimization" "Controller"
                projectionController = component "Projection Controller" "Projection CRUD and sync" "Controller"
                queryDSL = component "Query DSL" "Fluent, pipe-friendly Elixir query building" "Domain"
                projectionGenServer = component "Projection GenServer" "OTP-supervised real-time materialized views" "Application"
                pipelineProcessor = component "Pipeline Processor" "Composable event transformation chains" "Application"
                rustCoreClient = component "Rust Core Client" "Tesla HTTP client for Core communication" "Infrastructure"
                webSocketChannel = component "WebSocket Channel" "Real-time event subscriptions" "Infrastructure"
                broadwayProducer = component "Broadway Producer" "High-throughput parallel event processing" "Infrastructure"
            }

            # --- MCP Server ---
            mcpServer = container "MCP Server" "73 AI-native tool definitions for natural language interaction with events" "Elixir, JSON-RPC 2.0" "Service" {
                jsonRpcHandler = component "JSON-RPC Handler" "JSON-RPC 2.0 protocol over stdio" "Protocol"

                # Core Query Tools
                queryEventsTool = component "query_events" "Event retrieval with entity, type, time filters" "Tool"
                reconstructStateTool = component "reconstruct_state" "Point-in-time entity state reconstruction" "Tool"
                analyzeChangesTool = component "analyze_changes" "Change detection, diffs, and anomalies" "Tool"
                findPatternsTool = component "find_patterns" "Pattern recognition across event streams" "Tool"
                eventTimelineTool = component "event_timeline" "Chronological event visualization" "Tool"

                # Search Tools
                semanticSearchTool = component "semantic_search_events" "Semantic search over events" "Tool"
                hybridSearchTool = component "hybrid_search" "Combined text and semantic search" "Tool"

                # Management Tools
                deleteEventsTool = component "delete_events" "Soft delete with comprehensive audit trail" "Tool"
                archiveEventsTool = component "archive_events" "Cold storage migration for old events" "Tool"
                exportEventsTool = component "export_events" "Export to JSON, JSONL, CSV, Parquet" "Tool"
                importEventsTool = component "import_events" "Bulk import with deduplication" "Tool"
                cloneEntityTool = component "clone_entity" "Deep copy of entity event history" "Tool"
                mergeEntitiesTool = component "merge_entities" "Combine multiple entity streams" "Tool"

                toonFormatter = component "TOON Formatter" "Token-optimized output format (about 50% fewer tokens than JSON for tabular data)" "Service"
                sessionManager = component "Session Manager" "Context and conversation state tracking" "Service"
            }

            # --- Data Stores ---
            eventStorage = container "Event Storage" "Parquet columnar files with write-ahead log segments" "Parquet + WAL" "Database"
        }

        # ==================== Relationships ====================

        # Users -> System
        developer -> allsource "Ingests events, queries history, manages schemas"
        dataEngineer -> allsource "Creates projections, exports data, builds pipelines"
        aiEngineer -> allsource "Uses MCP tools for AI workflows"
        operator -> allsource "Configures tenants, monitors health, manages security"

        # External Systems -> AllSource
        clientApps -> allsource "Consumes events via REST/WebSocket"
        llmAgents -> allsource.mcpServer "Natural language queries via MCP protocol"
        allsource -> monitoringStack "Exports metrics (Prometheus) and traces (OTLP)"
        allsource.controlPlane -> identityProvider "Validates SSO tokens"

        # Users -> Containers
        developer -> webDashboard "Manages events via browser"
        aiEngineer -> mcpServer "Configures MCP for AI agents"

        # Container Relationships
        webDashboard -> controlPlane "API calls via HTTPS with JWT"
        controlPlane -> eventStoreCore "Proxies authenticated requests"
        controlPlane -> queryService "Routes query requests"
        mcpServer -> eventStoreCore "Fetches events via HTTP"
        mcpServer -> queryService "Executes queries via HTTP"

        eventStoreCore -> eventStorage "Persists events"

        # Control Plane Components
        ginRouter -> authHandler "Routes /api/v1/auth/*"
        ginRouter -> clusterHandler "Routes /api/v1/status/services"
        ginRouter -> tenantHandler "Routes /api/v1/tenants/*"
        ginRouter -> operationsHandler "Routes /api/v1/operations/*"
        authHandler -> jwtService "Generates and validates tokens"
        ginRouter -> rbacService "Checks permissions on each request"
        ginRouter -> auditService "Logs all operations"
        ginRouter -> tracingService "Creates distributed traces"
        ginRouter -> policyEngine "Enforces security policies"
        clusterHandler -> proxyService "Forwards to backend services"

        # Event Store Core Components
        restApi -> ingestEventUC "POST /events"
        restApi -> queryEventsUC "GET /events"
        restApi -> manageSchemaUC "Schema operations"
        restApi -> manageProjectionUC "Projection operations"
        restApi -> manageTenantUC "Tenant operations"

        ingestEventUC -> eventEntity "Creates"
        ingestEventUC -> schemaEntity "Validates against"
        queryEventsUC -> eventStream "Reads from"
        manageProjectionUC -> projectionEntity "Manages"
        manageTenantUC -> tenantEntity "Manages"

        ingestEventUC -> parquetStorage "Persists events"
        ingestEventUC -> writeAheadLog "Writes for durability"
        ingestEventUC -> dashMapIndex "Updates indexes"
        queryEventsUC -> dashMapIndex "Queries indexes"
        restApi -> jwtAuth "Authenticates requests"

        # Query Service Components
        phoenixRouter -> eventController "Routes /api/v1 event endpoints"
        phoenixRouter -> queryController "Routes /api/v1 query endpoints"
        phoenixRouter -> projectionController "Routes projection endpoints"
        queryController -> queryDSL "Builds queries"
        projectionController -> projectionGenServer "Manages real-time views"
        eventController -> pipelineProcessor "Transforms events"
        eventController -> rustCoreClient "Fetches from Core"
        projectionGenServer -> rustCoreClient "Syncs with Core"
        phoenixRouter -> webSocketChannel "Streams events"
        pipelineProcessor -> broadwayProducer "Parallel processing"

        # MCP Server Components
        jsonRpcHandler -> queryEventsTool "Dispatches query_events"
        jsonRpcHandler -> reconstructStateTool "Dispatches reconstruct_state"
        jsonRpcHandler -> analyzeChangesTool "Dispatches analyze_changes"
        jsonRpcHandler -> findPatternsTool "Dispatches find_patterns"
        jsonRpcHandler -> semanticSearchTool "Dispatches semantic_search_events"
        jsonRpcHandler -> deleteEventsTool "Dispatches delete_events"
        jsonRpcHandler -> exportEventsTool "Dispatches export_events"
        queryEventsTool -> toonFormatter "Formats responses"
        jsonRpcHandler -> sessionManager "Tracks conversation context"
    }

    views {
        # Level 1: System Context
        systemContext allsource "SystemContext" {
            include *
            autoLayout
        }

        # Level 2: Container
        container allsource "Containers" {
            include *
            autoLayout
        }

        # Level 3: Control Plane Components
        component controlPlane "ControlPlane_Components" {
            include *
            autoLayout
        }

        # Level 3: Event Store Core Components
        component eventStoreCore "EventStoreCore_Components" {
            include *
            autoLayout
        }

        # Level 3: Query Service Components
        component queryService "QueryService_Components" {
            include *
            autoLayout
        }

        # Level 3: MCP Server Components
        component mcpServer "MCPServer_Components" {
            include *
            autoLayout
        }

        styles {
            element "Person" {
                shape Person
                background #08427b
                color #ffffff
            }
            element "User" {
                shape Person
                background #08427b
                color #ffffff
            }
            element "Software System" {
                background #1168bd
                color #ffffff
            }
            element "External" {
                background #999999
                color #ffffff
            }
            element "Container" {
                background #438dd5
                color #ffffff
            }
            element "Service" {
                shape RoundedBox
                background #438dd5
                color #ffffff
            }
            element "Web Browser" {
                shape WebBrowser
                background #438dd5
                color #ffffff
            }
            element "Database" {
                shape Cylinder
                background #438dd5
                color #ffffff
            }
            element "Component" {
                background #85bbf0
                color #000000
            }
            element "Entity" {
                background #f5da81
                color #000000
            }
            element "Aggregate" {
                background #f5da81
                color #000000
            }
            element "Use Case" {
                background #c8e6c9
                color #000000
            }
            element "Infrastructure" {
                background #ffccbc
                color #000000
            }
            element "Tool" {
                background #e1bee7
                color #000000
            }
            element "Handler" {
                background #b3e5fc
                color #000000
            }
        }

        themes default
    }

    configuration {
        scope softwaresystem
    }
}
