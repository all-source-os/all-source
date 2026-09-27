defmodule QueryServiceEx.Application do
  # See https://hexdocs.pm/elixir/Application.html
  # for more information on OTP Applications
  @moduledoc false

  use Application

  alias QueryServiceEx.Application.Services.ProjectionSync
  alias QueryServiceEx.Cluster.Topology
  alias QueryServiceEx.Integrations.Kafka
  alias QueryServiceEx.Integrations.RabbitMQ
  alias QueryServiceEx.Projections.TenantProjections

  @impl true
  def start(_type, _args) do
    # Log warning if dev mode is enabled
    QueryServiceEx.DevMode.maybe_log_warning()

    # Attach telemetry handlers for structured logging
    QueryServiceEx.Telemetry.attach_handlers()

    # Initialize ETS cache for projections (legacy Core-state sync)
    ProjectionSync.init_cache()

    # Initialize ETS tables for the per-tenant (QS-owned) projections feature
    TenantProjections.init_tables()

    # Get cluster topology child_spec (nil if clustering disabled)
    cluster_children = cluster_children()

    children =
      [
        # PubSub for event broadcasting (must start before cluster components)
        {Phoenix.PubSub, name: QueryServiceEx.PubSub},

        # Prometheus metrics exporter (must start before other services emit telemetry)
        QueryServiceEx.PrometheusMetrics,

        # Rate limiter for per-tenant request throttling
        QueryServiceEx.RateLimiter,

        # Circuit breaker for Core backend calls
        {QueryServiceEx.CircuitBreaker, name: QueryServiceEx.CircuitBreaker},

        # ETS cache for tenant data (invalidated by Control Plane webhooks)
        QueryServiceEx.TenantCache,

        # ETS cache for verified API key lookups (120s TTL)
        QueryServiceEx.ApiKeyCache,

        # Async usage increment reporter (buffers and flushes to Core)
        QueryServiceEx.UsageReporter,

        # Own bounded review tasks through caller cancellation and dispatcher restarts.
        QueryServiceEx.Application.Services.CustomerReviewWork,

        # ETS cache for analytics results
        QueryServiceEx.Infrastructure.Adapters.AnalyticsCache,

        # ETS-backed team membership store (projects team events from Core)
        QueryServiceEx.TeamStore,

        # ETS-backed API key metadata store
        QueryServiceEx.ApiKeyStore,

        # Admin metrics cache (15s TTL, time-series ring buffer)
        QueryServiceEx.MetricsCache,

        # Health checker for Core read nodes (polls /health, tracks lag in ETS)
        QueryServiceEx.Infrastructure.Adapters.CoreHealthChecker,

        # Replay lag monitor (polls Core stats, tracks event count lag)
        QueryServiceEx.Infrastructure.Adapters.ReplayLagMonitor,

        # Daily demo data reset (disabled unless DEMO_RESET_ENABLED=true)
        QueryServiceEx.DemoReset
      ] ++
        cluster_children ++
        integration_children() ++
        [
          # Registry for projection sync processes (local fallback when clustering disabled)
          {Registry, keys: :unique, name: QueryServiceEx.ProjectionRegistry},

          # DynamicSupervisor for projection sync processes (local fallback)
          {DynamicSupervisor,
           strategy: :one_for_one, name: QueryServiceEx.ProjectionSyncSupervisor},

          # DynamicSupervisor for server-side projection servers (Phase 2 continuous projections)
          QueryServiceEx.Projections.ProjectionSupervisor,

          # Task supervisor for per-tenant projection background backfills
          {Task.Supervisor, name: QueryServiceEx.Projections.BackfillSupervisor},

          # Per-tenant (QS-owned) projection state owner + live folder
          QueryServiceEx.Projections.TenantProjections,

          # WebSocket client for real-time events from Core
          {QueryServiceEx.Infrastructure.Adapters.CoreWebSocketClient, []},

          # Presence for tracking connected WebSocket clients
          QueryServiceExWeb.Presence,

          # Start the Phoenix endpoint
          QueryServiceExWeb.Endpoint
        ]

    # See https://hexdocs.pm/elixir/Supervisor.html
    # for other strategies and supported options
    opts = [strategy: :one_for_one, name: QueryServiceEx.Supervisor]
    Supervisor.start_link(children, opts)
  end

  # Tell Phoenix to update the endpoint configuration
  # whenever the application is updated.
  @impl true
  def config_change(changed, _new, removed) do
    QueryServiceExWeb.Endpoint.config_change(changed, removed)
    :ok
  end

  # Returns message queue integration children (Kafka, RabbitMQ)
  # Only started when explicitly enabled via config or env vars
  defp integration_children do
    children = []

    # Add Kafka producer if enabled
    children =
      if Kafka.Producer.enabled?() do
        [Kafka.Producer | children]
      else
        children
      end

    # Add Kafka consumer if enabled
    children =
      if Kafka.Consumer.enabled?() do
        [Kafka.Consumer | children]
      else
        children
      end

    # Add RabbitMQ producer if enabled
    children =
      if RabbitMQ.Producer.enabled?() do
        [RabbitMQ.Producer | children]
      else
        children
      end

    # Add RabbitMQ consumer if enabled
    if RabbitMQ.Consumer.enabled?() do
      [RabbitMQ.Consumer | children]
    else
      children
    end
  end

  # Returns cluster-related children based on CLUSTER_STRATEGY env var
  defp cluster_children do
    cluster_topology = Topology.child_spec()

    base_cluster = [
      # Consistent hash ring for entity-to-node mapping
      QueryServiceEx.Cluster.ConsistentHash,

      # Distributed registry for projection processes
      QueryServiceEx.Cluster.DistributedRegistry,

      # Distributed supervisor for projection processes
      QueryServiceEx.Cluster.DistributedSupervisor,

      # Cluster membership manager
      QueryServiceEx.Cluster.Membership
    ]

    if cluster_topology do
      # Add libcluster supervisor when clustering is enabled
      [cluster_topology | base_cluster]
    else
      # Just start the cluster components (they work in single-node mode too)
      base_cluster
    end
  end
end
