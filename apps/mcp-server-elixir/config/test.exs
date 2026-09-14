import Config

# Print only warnings and errors during test
config :logger, level: :warning

# Use simple console format for tests
config :logger, :console,
  format: "[$level] $message\n",
  metadata: []

# Disable external connections in tests
config :mcp_server_elixir,
  core_ws_enabled: false,
  event_pipeline_enabled: false,
  start_stdio_server: false
