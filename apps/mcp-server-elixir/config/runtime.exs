import Config

config :mcp_server_elixir,
  customer_review: System.get_env("ALLSOURCE_CUSTOMER_REVIEW") == "true",
  customer_evidence_review: System.get_env("ALLSOURCE_CUSTOMER_EVIDENCE_REVIEW") == "true",
  customer_replay_review: System.get_env("ALLSOURCE_CUSTOMER_REPLAY_REVIEW") == "true",
  customer_review_connection_file: System.get_env("CUSTOMER_REVIEW_CONNECTION_FILE"),
  customer_review_http: System.get_env("ALLSOURCE_CUSTOMER_REVIEW_HTTP") == "true",
  customer_review_query_url: System.get_env("CUSTOMER_REVIEW_QUERY_URL"),
  customer_review_metadata_url: System.get_env("CUSTOMER_REVIEW_METADATA_URL"),
  customer_review_http_port: String.to_integer(System.get_env("CUSTOMER_REVIEW_HTTP_PORT", "3904")),
  customer_review_http_ip: System.get_env("CUSTOMER_REVIEW_HTTP_IP", "127.0.0.1"),
  customer_review_origins:
    String.split(System.get_env("CUSTOMER_REVIEW_ORIGINS", ""), ",", trim: true)

# Runtime config — these env vars are read when the release STARTS, not at compile time.
# This is critical for Docker deployments where env vars are set at container run time.

# Core API key for authenticated connections (fixes #109)
# Accepts CORE_API_KEY or ALLSOURCE_CORE_API_KEY (backward compat with config.exs)
if api_key = System.get_env("CORE_API_KEY") || System.get_env("ALLSOURCE_CORE_API_KEY") do
  config :mcp_server_elixir, core_api_key: api_key
end

# Core URL overrides (runtime)
if core_url = System.get_env("CORE_URL") || System.get_env("ALLSOURCE_CORE_URL") do
  config :mcp_server_elixir, core_url: core_url
end

if core_ws_url = System.get_env("CORE_WS_URL") || System.get_env("ALLSOURCE_CORE_WS_URL") do
  config :mcp_server_elixir, core_ws_url: core_ws_url
end

if control_url = System.get_env("CONTROL_URL") || System.get_env("ALLSOURCE_CONTROL_URL") do
  config :mcp_server_elixir, control_url: control_url
end

if read_only = System.get_env("ALLSOURCE_READ_ONLY") do
  config :mcp_server_elixir, read_only: read_only == "true"
end

core_mode = System.get_env("CORE_MODE", "remote")

case core_mode do
  "embedded" ->
    config :mcp_server_elixir,
      core_mode: :embedded,
      core_backend: McpServerElixir.Infrastructure.CoreEmbedded,
      core_ws_enabled: false,
      embedded_config: %{
        data_dir: System.get_env("CORE_DATA_DIR", ""),
        node_id: String.to_integer(System.get_env("CORE_NODE_ID", "1")),
        wal_fsync_interval_ms: String.to_integer(System.get_env("CORE_WAL_FSYNC_MS", "100")),
        parquet_flush_interval_secs:
          String.to_integer(System.get_env("CORE_PARQUET_FLUSH_SECS", "300"))
      }

  _ ->
    config :mcp_server_elixir,
      core_mode: :remote,
      core_backend: McpServerElixir.Infrastructure.CoreClient
end
