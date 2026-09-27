defmodule QueryServiceExWeb.Router do
  @moduledoc """
  Phoenix router for Query Service API endpoints.

  Routes are organized into:
  - Public routes (health checks, dev token)
  - Authenticated routes (require JWT token)
  - Tenant-scoped routes (require auth + active subscription)
  """

  use Phoenix.Router

  import Plug.Conn
  import Phoenix.Controller

  # -------------------------------------------------------------------
  # Pipelines
  # -------------------------------------------------------------------

  pipeline :api do
    plug(:accepts, ["json"])
    plug(QueryServiceExWeb.Plugs.CorrelationId)
  end

  pipeline :authenticated do
    plug(:accepts, ["json"])
    plug(QueryServiceExWeb.Plugs.CorrelationId)
    plug(QueryServiceExWeb.Plugs.AuthPipeline)
  end

  # Requires auth + tenant context + active subscription
  pipeline :tenant_scoped do
    plug(:accepts, ["json"])
    plug(QueryServiceExWeb.Plugs.CorrelationId)
    plug(QueryServiceExWeb.Plugs.AuthPipeline)
    plug(QueryServiceExWeb.Plugs.TenantContext)
    plug(QueryServiceExWeb.Plugs.ConsistencyRouting)
  end

  # Requires tenant + enforces event usage quota
  pipeline :events_quota do
    plug(QueryServiceExWeb.Plugs.UsageEnforcement, type: :events)
  end

  # Requires tenant + enforces query usage quota
  pipeline :queries_quota do
    plug(QueryServiceExWeb.Plugs.UsageEnforcement, type: :queries)
  end

  # Rate limiting to prevent any single tenant from overwhelming the system
  pipeline :rate_limited do
    plug(QueryServiceExWeb.Plugs.RateLimiting)
  end

  # API key authentication (X-API-Key header) — replaces AuthPipeline + TenantContext
  pipeline :api_key_authenticated do
    plug(:accepts, ["json"])
    plug(QueryServiceExWeb.Plugs.CorrelationId)
    plug(QueryServiceExWeb.Plugs.ApiKeyAuth)
    plug(QueryServiceExWeb.Plugs.ConsistencyRouting)
  end

  # Internal service-to-service auth via shared INTERNAL_API_KEY
  pipeline :internal_authenticated do
    plug(QueryServiceExWeb.Plugs.InternalApiKey)
  end

  # -------------------------------------------------------------------
  # Public Routes
  # -------------------------------------------------------------------

  # Root health check (MCP expects /health)
  scope "/", QueryServiceExWeb do
    pipe_through(:api)

    get("/health", HealthController, :show)
  end

  # Health checks (no auth required)
  scope "/api", QueryServiceExWeb do
    pipe_through(:api)

    get("/health", HealthController, :show)
    get("/health/live", HealthController, :live)
    get("/health/ready", HealthController, :ready)
    get("/health/replay", HealthController, :replay)

    # Metrics endpoints
    # GET /api/metrics - JSON format (default) or Prometheus (with Accept header/format param)
    get("/metrics", MetricsController, :show)
    # GET /api/metrics/prometheus - Always returns Prometheus text format
    get("/metrics/prometheus", MetricsController, :prometheus)

    # OpenAPI documentation
    # GET /api/openapi - Returns OpenAPI 3.0 specification in JSON
    get("/openapi", OpenApiController, :spec)
    # GET /api/docs - Swagger UI for interactive API documentation
    get("/docs", OpenApiController, :docs)

    # Cluster health (public endpoint)
    get("/cluster", ClusterController, :health)
    get("/cluster/members", ClusterController, :members)
  end

  # Cluster management (authenticated)
  scope "/api/cluster", QueryServiceExWeb do
    pipe_through(:authenticated)

    get("/registry", ClusterController, :registry)
    get("/supervisor", ClusterController, :supervisor)
    get("/hash-ring", ClusterController, :hash_ring)
  end

  # Demo endpoints (no auth — demo-friendly)
  scope "/api/v1/demo", QueryServiceExWeb do
    pipe_through(:api)

    post("/seed", DemoController, :seed)
  end

  # Feedback endpoint (no auth — demo-friendly)
  scope "/api/v1", QueryServiceExWeb do
    pipe_through(:api)

    post("/feedback", DemoController, :feedback)
  end

  # Config endpoints (no auth — public config data)
  scope "/api/v1/config", QueryServiceExWeb do
    pipe_through(:api)

    get("/benchmarks", ConfigController, :benchmarks)
  end

  # Public auth routes (no authentication required)
  scope "/api/auth", QueryServiceExWeb do
    pipe_through(:api)

    # Dev token endpoint — excluded from router entirely in prod builds.
    # In non-prod, the controller still checks AUTH_DISABLED at runtime.
    if Mix.env() != :prod do
      get("/dev-token", AuthController, :dev_token)
    end
  end

  # -------------------------------------------------------------------
  # Authenticated Routes (user context, no tenant required)
  # -------------------------------------------------------------------

  # Agent grants have their own live boundary. Generic JWT/dev/tenant fallback
  # pipelines must not authenticate this surface. Never log proposal parameters.
  scope "/api/customer-agent", QueryServiceExWeb do
    pipe_through(:api)

    post("/context", CustomerAgentController, :context, log: false)
    post("/validate", CustomerAgentController, :validate, log: false)
    post("/prepare", CustomerAgentController, :prepare, log: false)
    post("/review", CustomerAgentController, :review, log: false)
    post("/result", CustomerAgentController, :result, log: false)
    post("/connections/list", CustomerConnectionsController, :index, log: false)
    post("/connections/create", CustomerConnectionsController, :create, log: false)
    post("/connections/revoke", CustomerConnectionsController, :revoke, log: false)
    post("/connections/share", CustomerConnectionsController, :share, log: false)
    post("/connections/authorize", CustomerOAuthController, :authorize, log: false)
    get("/oauth/metadata", CustomerOAuthController, :metadata, log: false)
    get("/oauth/resource", CustomerOAuthController, :resource, log: false)
    post("/oauth/prepare", CustomerOAuthController, :prepare, log: false)
    post("/oauth/inspect", CustomerOAuthController, :inspect_request, log: false)
    post("/oauth/token", CustomerOAuthController, :token, log: false)
    post("/remote/context", CustomerAgentController, :remote_context, log: false)
    post("/remote/session", CustomerAgentController, :remote_session, log: false)
    post("/remote/prepare", CustomerAgentController, :remote_prepare, log: false)
    post("/remote/review", CustomerAgentController, :remote_review, log: false)
    post("/remote/result", CustomerAgentController, :remote_result, log: false)
    post("/remote/validate", CustomerAgentController, :remote_validate, log: false)
  end

  scope "/api/auth", QueryServiceExWeb do
    pipe_through(:authenticated)

    get("/me", AuthController, :me)
    post("/logout", AuthController, :logout)
  end

  # v1 aliases for the session endpoints. The branded gateway (api.all-source.xyz)
  # routes /api/v1/* to the Query Service but NOT /api/auth/*. The web login
  # callback validates the session token via ${NEXT_PUBLIC_API_URL}/api/v1/auth/me;
  # without this it 404'd on api.all-source.xyz and every login showed
  # "Session expired". Same handlers as /api/auth above.
  scope "/api/v1/auth", QueryServiceExWeb do
    pipe_through(:authenticated)

    get("/me", AuthController, :me)
    post("/logout", AuthController, :logout)
  end

  # -------------------------------------------------------------------
  # Tenant-Scoped Routes (requires active subscription)
  # -------------------------------------------------------------------

  # Events endpoints (with usage quota enforcement for writes)
  scope "/api", QueryServiceExWeb do
    pipe_through([:tenant_scoped, :rate_limited, :events_quota])

    post("/events", EventController, :create)
    post("/events/batch", EventController, :create_batch)
  end

  # Events read endpoints (no quota for reads)
  scope "/api", QueryServiceExWeb do
    pipe_through([:tenant_scoped, :rate_limited])

    get("/events", EventController, :index)
    get("/events/query", EventController, :query_core_compat)
    get("/events/:id", EventController, :show)
    get("/events/entity/:entity_id", EventController, :by_entity)
    get("/events/type/:event_type", EventController, :by_type)

    # Discovery endpoints for streams and event types
    get("/streams", EventController, :streams)
    get("/event-types", EventController, :event_types)
  end

  # Query execution (with usage quota enforcement)
  scope "/api", QueryServiceExWeb do
    pipe_through([:tenant_scoped, :rate_limited, :queries_quota])

    post("/query", QueryController, :execute)
    post("/query/projected", ProjectedQueryController, :execute)
  end

  # Per-tenant projections (QS-owned; enable/disable curated templates).
  # Tenant-scoped + fail-closed. No quota for projection operations.
  scope "/api", QueryServiceExWeb do
    pipe_through([:tenant_scoped, :rate_limited])

    get("/projections", ProjectionController, :index)
    get("/projection-templates", ProjectionController, :templates)
    post("/projections", ProjectionController, :create)
    delete("/projections/:name", ProjectionController, :delete)
    get("/projections/:name/state", ProjectionController, :get_state)
  end

  # Webhook subscription management endpoints
  scope "/api", QueryServiceExWeb do
    pipe_through([:tenant_scoped, :rate_limited])

    get("/webhooks", WebhookSubscriptionController, :index)
    post("/webhooks", WebhookSubscriptionController, :create)
    get("/webhooks/:id", WebhookSubscriptionController, :show)
    put("/webhooks/:id", WebhookSubscriptionController, :update)
    delete("/webhooks/:id", WebhookSubscriptionController, :delete)
    get("/webhooks/:id/deliveries", WebhookSubscriptionController, :deliveries)
  end

  # Schema endpoints
  scope "/api", QueryServiceExWeb do
    pipe_through([:tenant_scoped, :rate_limited])

    get("/schemas", SchemaController, :index)
    get("/schemas/:event_type", SchemaController, :show)
    post("/schemas", SchemaController, :register)
  end

  # Tenant-scoped, Query Service-owned projection rebuilds
  scope "/api", QueryServiceExWeb do
    pipe_through([:tenant_scoped, :rate_limited])

    get("/replay", ReplayController, :index)
    post("/replay/preview", ReplayController, :preview)
    post("/replay", ReplayController, :create)
    get("/replay/:id", ReplayController, :show)
    post("/replay/:id/cancel", ReplayController, :cancel)
    delete("/replay/:id", ReplayController, :delete)
  end

  # -------------------------------------------------------------------
  # Analytics Routes
  # -------------------------------------------------------------------

  # Analytics endpoints (quota enforced for complex queries)
  scope "/api/analytics", QueryServiceExWeb do
    pipe_through([:tenant_scoped, :rate_limited, :queries_quota])

    # Core analytics (proxied to Rust Core)
    get("/frequency", AnalyticsController, :frequency)
    get("/summary", AnalyticsController, :summary)
    get("/correlation", AnalyticsController, :correlation)

    # Extended analytics (computed locally)
    get("/percentiles", AnalyticsController, :percentiles)
    get("/stddev", AnalyticsController, :stddev)

    # Time window aggregations
    get("/sliding-window", AnalyticsController, :sliding_window)
    get("/session-window", AnalyticsController, :session_window)

    # Cache management
    get("/cache/stats", AnalyticsController, :cache_stats)
    post("/cache/invalidate", AnalyticsController, :invalidate_cache)
  end

  # -------------------------------------------------------------------
  # Integrations Routes (Message Queues)
  # -------------------------------------------------------------------

  # Public status endpoints
  scope "/api/integrations", QueryServiceExWeb do
    pipe_through(:api)

    get("/", IntegrationsController, :all_status)
    get("/kafka", IntegrationsController, :kafka_status)
    get("/rabbitmq", IntegrationsController, :rabbitmq_status)
  end

  # Authenticated management endpoints
  scope "/api/integrations", QueryServiceExWeb do
    pipe_through(:authenticated)

    get("/kafka/config", IntegrationsController, :kafka_config)
    post("/kafka/publish", IntegrationsController, :kafka_publish)
    get("/rabbitmq/config", IntegrationsController, :rabbitmq_config)
    post("/rabbitmq/publish", IntegrationsController, :rabbitmq_publish)
  end

  # -------------------------------------------------------------------
  # Admin Metrics Routes
  # -------------------------------------------------------------------

  scope "/api/admin/metrics", QueryServiceExWeb do
    pipe_through(:authenticated)

    get("/summary", AdminMetricsController, :summary)
    get("/timeseries", AdminMetricsController, :timeseries)
  end

  # -------------------------------------------------------------------
  # API Key Management Routes
  # -------------------------------------------------------------------

  scope "/api", QueryServiceExWeb do
    pipe_through(:authenticated)

    get("/api-keys", ApiKeyController, :index)
    post("/api-keys", ApiKeyController, :create)
    delete("/api-keys/:id", ApiKeyController, :revoke)
  end

  # -------------------------------------------------------------------
  # Tenant Management Routes (enterprise only)
  # -------------------------------------------------------------------

  if QueryServiceEx.Edition.enterprise?() do
    scope "/api", QueryServiceExWeb do
      pipe_through(:authenticated)

      # Tenant info and settings
      get("/tenant", TenantController, :show)
      put("/tenant", TenantController, :update)

      # Usage statistics
      get("/tenant/usage", TenantController, :usage)

      # Schema-enforcement toggle (Gap 3) — proxies Core's per-tenant setting.
      get("/tenant/schema-enforcement", TenantController, :get_schema_enforcement)
      put("/tenant/schema-enforcement", TenantController, :set_schema_enforcement)
    end

    # -------------------------------------------------------------------
    # Team Management Routes (enterprise only)
    # -------------------------------------------------------------------

    scope "/api/team", QueryServiceExWeb do
      pipe_through(:authenticated)

      get("/members", TeamController, :index)
      post("/invite", TeamController, :invite)
      delete("/members/:user_id", TeamController, :remove)
      put("/members/:user_id/role", TeamController, :update_role)
    end

    # -------------------------------------------------------------------
    # Audit Log Routes (enterprise only)
    # -------------------------------------------------------------------

    scope "/api/tenant", QueryServiceExWeb do
      pipe_through(:authenticated)

      get("/audit-logs", AuditLogController, :index)
    end

    # -------------------------------------------------------------------
    # Usage Analytics Routes (enterprise only)
    # -------------------------------------------------------------------

    scope "/api/tenants/me", QueryServiceExWeb do
      pipe_through(:authenticated)

      get("/analytics", UsageAnalyticsController, :show)
    end
  end

  # -------------------------------------------------------------------
  # Billing Routes (enterprise only — disabled in community edition)
  # -------------------------------------------------------------------

  if QueryServiceEx.Edition.enterprise?() do
    # Billing status reads a TENANT'S billing events out of Core, so it needs a
    # tenant context. On the `:api` pipeline it never got one: `assigns.tenant_id`
    # was always nil and `BillingController.status/2` raised a FunctionClauseError
    # inside RustCoreClient (`query_events/3` guards on `is_binary(tenant_id)`) —
    # a hard 500 on every request, which is the "HTTP 500 (Core proxy error)"
    # recorded in docs/checklists/dashboard-data-audit.md. `:authenticated`, not
    # `:tenant_scoped`: gating billing status on an active subscription would lock
    # a lapsed tenant out of the very page that tells them their subscription
    # lapsed.
    scope "/api/billing", QueryServiceExWeb do
      pipe_through(:authenticated)

      get("/status", BillingController, :status)
    end

    scope "/api/billing", QueryServiceExWeb do
      pipe_through(:api)

      # These redirect to Control Plane (need LemonSqueezy API access)
      get("/portal", BillingController, :portal)
      post("/checkout", BillingController, :checkout)
      get("/overage", BillingController, :overage)
      post("/overage/enable", BillingController, :enable_overage)
      post("/overage/disable", BillingController, :disable_overage)
      get("/projected-charges", BillingController, :projected_charges)
    end
  end

  # -------------------------------------------------------------------
  # Internal Routes (service-to-service, no auth)
  # -------------------------------------------------------------------

  # Called by allsource-sentinel during failover to update the leader URL
  scope "/internal", QueryServiceExWeb do
    pipe_through(:api)

    post("/update-leader", InternalController, :update_leader)
  end

  # Authenticated internal endpoints (require INTERNAL_API_KEY)
  scope "/internal", QueryServiceExWeb do
    pipe_through([:api, :internal_authenticated])

    post("/tenant-updated", InternalController, :tenant_updated)
  end

  # -------------------------------------------------------------------
  # MCP-Compatible v1 API Routes
  # -------------------------------------------------------------------
  # The chronos-mcp Docker container expects /api/v1/ prefixed URLs.
  # These routes use the tenant_scoped pipeline which bypasses auth
  # in dev mode (AUTH_DISABLED=true) and injects the dev tenant.

  # Health check (no auth)
  scope "/api/v1", QueryServiceExWeb do
    pipe_through(:api)

    get("/health", HealthController, :show)
  end

  # Event read endpoints (tenant-scoped for data isolation)
  scope "/api/v1", QueryServiceExWeb do
    pipe_through([:tenant_scoped, :rate_limited])

    get("/events", EventController, :index)
    get("/events/query", EventController, :query_core_compat)
    get("/events/recent", EventController, :recent)
    get("/events/:id", EventController, :show)
    get("/events/entity/:entity_id", EventController, :by_entity)
    get("/events/type/:event_type", EventController, :by_type)
    get("/streams", EventController, :streams)
    get("/event-types", EventController, :event_types)
  end

  # Event write endpoints (v1). The TypeScript SDK (@allsource/client) posts to
  # /api/v1/events and /api/v1/events/batch; without these aliases the SDK 404s
  # on every write. Mirrors the unversioned /api/events write routes, same
  # quota + rate-limit pipeline.
  scope "/api/v1", QueryServiceExWeb do
    pipe_through([:tenant_scoped, :rate_limited, :events_quota])

    post("/events", EventController, :create)
    post("/events/batch", EventController, :create_batch)
  end

  # Snapshot endpoints (tenant-scoped)
  scope "/api/v1", QueryServiceExWeb do
    pipe_through([:tenant_scoped, :rate_limited])

    get("/snapshots", SnapshotController, :index)
  end

  # Per-tenant projections (QS-owned), /api/v1 mirror of the /api routes.
  scope "/api/v1", QueryServiceExWeb do
    pipe_through([:tenant_scoped, :rate_limited])

    get("/projections", ProjectionController, :index)
    get("/projection-templates", ProjectionController, :templates)
    post("/projections", ProjectionController, :create)
    delete("/projections/:name", ProjectionController, :delete)
    get("/projections/:name/state", ProjectionController, :get_state)
  end

  # Prime declarative projections + per-field provenance (tenant-scoped).
  # Proxies Core's internal /api/v1/prime/* projection routes (t-2ac8).
  scope "/api/v1/prime", QueryServiceExWeb do
    pipe_through([:tenant_scoped, :rate_limited])

    get("/graph", PrimeController, :graph)
    get("/projections", PrimeController, :index)
    post("/projections", PrimeController, :create)
    post("/nodes/:id/project", PrimeController, :project)
    get("/nodes/:id/fields/:field/provenance", PrimeController, :provenance)
  end
end
