import Config

# Configure structured logging defaults
# Include all metadata keys used across the application
config :logger, :console,
  metadata: [
    # Standard metadata
    :request_id,
    :correlation_id,
    :entity_id,
    :event_type,
    :module,
    :function,
    # Auth and user context
    :user_id,
    :tenant_id,
    :provider,
    :api_key_id,
    :name,
    :old_api_key_id,
    :new_api_key_id,
    # Billing metadata
    :customer_id,
    :variant_id,
    :subscription_id,
    :status,
    :usage_type,
    :overage_count,
    :count,
    :percentage,
    :threshold,
    :reason,
    :event_name,
    # Error handling metadata
    :error,
    :kind,
    :stacktrace,
    :exception_type,
    :error_type,
    # Telemetry and metrics
    :telemetry_event,
    :duration_ms,
    :source,
    :queue_time_ms,
    :decode_time_ms,
    :method,
    :path,
    :event_id,
    :processing_time_ms,
    :projection_count,
    :sync_time_ms,
    :event_ids,
    :projection_name,
    # WebSocket metadata
    :url,
    :message_type,
    :message_size_bytes,
    :attempt,
    :backoff_ms,
    :attempts,
    :last_error,
    :total_reconnects,
    :reconnect_attempts,
    # Circuit breaker metadata
    :circuit,
    :from_state,
    :failure_count,
    # Health check metadata
    :check,
    :timeout_ms,
    :core_available,
    :failed_syncs,
    # Step tracking
    :step
  ]

# Configure the Phoenix endpoint
config :query_service_ex, QueryServiceExWeb.Endpoint,
  url: [host: "localhost"],
  adapter: Bandit.PhoenixAdapter,
  render_errors: [
    formats: [json: QueryServiceExWeb.ErrorJSON],
    layout: false
  ],
  pubsub_server: QueryServiceEx.PubSub,
  live_view: [signing_salt: "query_service_secret"],
  http: [port: 3902],
  server: true

# Configure Rust Core backend URLs and authentication
config :query_service_ex,
  core_url: System.get_env("CORE_URL") || "http://localhost:3900",
  core_ws_url: System.get_env("CORE_WS_URL") || "ws://localhost:3900/api/v1/events/stream",
  core_api_key: System.get_env("CORE_API_KEY")

# Configure Tesla HTTP client
config :query_service_ex,
  customer_agent_access_store: QueryServiceEx.Infrastructure.Adapters.CustomerAgentAccessStore,
  customer_connection_store: QueryServiceEx.Infrastructure.Adapters.CustomerAgentGrantStore,
  customer_authorization_code: QueryServiceEx.Infrastructure.Adapters.CustomerAuthorizationCode,
  customer_remote_tokens: QueryServiceEx.Infrastructure.Adapters.CustomerRemoteTokens,
  agent_run_source: QueryServiceEx.Infrastructure.Adapters.AgentRunStore,
  agent_run_writer: QueryServiceEx.Infrastructure.Adapters.AgentRunStore

config :query_service_ex,
  customer_review_store: QueryServiceEx.Infrastructure.Adapters.CustomerReviewStore,
  customer_query_operation_store:
    QueryServiceEx.Infrastructure.Adapters.CustomerQueryOperationStore,
  customer_query_usage_store: QueryServiceEx.Infrastructure.Adapters.CustomerQueryUsageStore

config :tesla,
  disable_deprecated_builder_warning: true

# Configure JSON encoding
config :phoenix, :json_library, Jason

# JWT shared secret is configured via JWT_SECRET env var (see runtime.exs)

# Edition: :community (default) or :enterprise
# Community mode disables tenant management, quota enforcement, and billing.
#
# This is read at COMPILE time because the router gates whole route scopes behind
# `if QueryServiceEx.Edition.enterprise?()` at module level (e.g. the
# /api/tenants/me/analytics, team, and audit-log routes), which Phoenix evaluates
# when the router module is compiled. runtime.exs also sets `:edition` at boot, but
# that is too late to bring compiled-out routes back. So the build must compile with
# the right edition: set ALLSOURCE_EDITION before `mix compile` (the Dockerfile
# exports the build ARG as ENV; fly.toml passes it as a build arg). Default stays
# :community.
config :query_service_ex,
       :edition,
       System.get_env("ALLSOURCE_EDITION", "community")
       |> String.downcase()
       |> (case do
             "enterprise" -> :enterprise
             _ -> :community
           end)

# Configure rate limiting (requests per second)
config :query_service_ex, QueryServiceEx.RateLimiter,
  default_rate: 100,
  default_burst: 200,
  cleanup_interval: 60_000,
  tier_limits: %{
    # Canonical 011 tiers (control-plane entities/subscription.go):
    # free → indie → studio → scale → enterprise.
    free: %{rate: 10, burst: 20},
    indie: %{rate: 100, burst: 200},
    studio: %{rate: 300, burst: 600},
    scale: %{rate: 1000, burst: 2000},
    enterprise: %{rate: 1000, burst: 2000},
    # Retired ids, normalized upstream; kept so not-yet-migrated tenants get a
    # sensible bucket and remain existing atoms.
    pro: %{rate: 300, burst: 600},
    growth: %{rate: 300, burst: 600},
    starter: %{rate: 100, burst: 200},
    developer: %{rate: 100, burst: 200},
    team: %{rate: 300, burst: 600}
  }

# Import environment specific config
import_config "#{config_env()}.exs"
