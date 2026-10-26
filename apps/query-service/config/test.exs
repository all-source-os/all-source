import Config

# We don't run a server during test
config :query_service_ex, QueryServiceExWeb.Endpoint,
  http: [ip: {127, 0, 0, 1}, port: 4002],
  secret_key_base: "test_secret_key_base_at_least_64_bytes_long_for_security_testing_purposes",
  server: false

# Disable WebSocket client during tests (Core may not be running)
config :query_service_ex, core_ws_enabled: false

# Run tests in enterprise mode so all routes (billing, webhooks) are compiled
# Individual tests can override at runtime if they need community mode behavior
config :query_service_ex, edition: :enterprise

# Allow all log levels during test (tests use capture_log and set level dynamically)
config :logger, level: :debug

# Use console format with all metadata for tests (tests rely on metadata being present)
config :logger, :console,
  format: "[$level] $metadata$message\n",
  metadata: [
    # Telemetry test metadata
    :method,
    :path,
    :status,
    :duration_ms,
    :event_id,
    :event_type,
    :entity_id,
    :projection_name,
    :projection_count,
    :url,
    :reason,
    :circuit,
    :from_state,
    :failure_count,
    :check,
    :timeout_ms,
    :error_type,
    :correlation_id,
    :request_id,
    :error,
    :attempts,
    :attempt,
    :backoff_ms,
    :last_error,
    :message_type,
    :message_size_bytes
  ]

# Initialize plugs at runtime for faster test compilation
config :phoenix, :plug_init_mode, :runtime
