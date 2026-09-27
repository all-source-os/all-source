import Config

# Customer review remains opt-in until connection consent and release gates pass.
config :query_service_ex,
  customer_review_enabled: System.get_env("CUSTOMER_REVIEW_ENABLED") == "true",
  customer_connections_enabled: System.get_env("CUSTOMER_CONNECTIONS_ENABLED") == "true",
  customer_remote_enabled: System.get_env("CUSTOMER_REMOTE_ENABLED") == "true",
  customer_oauth_issuer: System.get_env("CUSTOMER_OAUTH_ISSUER"),
  customer_review_resource: System.get_env("CUSTOMER_REVIEW_RESOURCE")

# Runtime configuration for production
# All environment variables are optional at startup - the app will fail gracefully
# when trying to use unconfigured services.
if config_env() == :prod do
  # Edition: "community" (default) or "enterprise"
  edition = System.get_env("ALLSOURCE_EDITION", "community")
  config :query_service_ex, :edition, String.to_atom(edition)

  # Secret key base - use provided value or generate a temporary one for health checks
  secret_key_base =
    System.get_env("SECRET_KEY_BASE") ||
      Base.encode64(:crypto.strong_rand_bytes(64))

  host = System.get_env("PHX_HOST") || "localhost"
  port = String.to_integer(System.get_env("PORT") || "3902")

  # Allowed origins for both WebSocket (check_origin) and HTTP (CORSPlug).
  # The frontend at www.all-source.xyz connects cross-origin to the QS.
  allowed_origins =
    case System.get_env("ALLOWED_ORIGINS") do
      nil -> ["https://www.all-source.xyz", "https://all-source.xyz"]
      origins -> origins |> String.split(",", trim: true) |> Enum.map(&String.trim/1)
    end

  config :query_service_ex, cors_origins: allowed_origins

  config :query_service_ex, QueryServiceExWeb.Endpoint,
    url: [host: host, port: 443, scheme: "https"],
    http: [
      # Enable IPv6 and bind on all interfaces.
      ip: {0, 0, 0, 0, 0, 0, 0, 0},
      port: port
    ],
    secret_key_base: secret_key_base,
    check_origin: allowed_origins

  # Configure Rust Core backend URLs and authentication
  #
  # For leader-follower replication:
  #   CORE_WRITE_URL  — URL of the leader node (writes always go here)
  #   CORE_READ_URLS  — comma-separated list of all node URLs for round-robin reads
  #
  # Backwards compatible: if neither is set, CORE_URL is used for both reads and writes.
  core_url = System.get_env("CORE_URL") || "http://localhost:3900"

  core_write_url = System.get_env("CORE_WRITE_URL") || core_url

  core_read_urls =
    case System.get_env("CORE_READ_URLS") do
      nil -> [core_url]
      "" -> [core_url]
      urls -> urls |> String.split(",", trim: true) |> Enum.map(&String.trim/1)
    end

  core_max_replication_lag_ms =
    String.to_integer(System.get_env("CORE_MAX_REPLICATION_LAG_MS") || "5000")

  core_health_check_interval_ms =
    String.to_integer(System.get_env("CORE_HEALTH_CHECK_INTERVAL_MS") || "5000")

  config :query_service_ex,
    core_url: core_url,
    core_write_url: core_write_url,
    core_read_urls: core_read_urls,
    core_ws_url: System.get_env("CORE_WS_URL") || "ws://localhost:3900/api/v1/events/stream",
    core_ws_enabled: System.get_env("CORE_WS_ENABLED", "true") == "true",
    core_api_key: System.get_env("CORE_API_KEY"),
    core_max_replication_lag_ms: core_max_replication_lag_ms,
    core_health_check_interval_ms: core_health_check_interval_ms
end
