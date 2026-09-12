defmodule McpServerElixir.MixProject do
  use Mix.Project

  def project do
    [
      app: :mcp_server_elixir,
      version: "0.24.0",
      elixir: "~> 1.17",
      elixirc_paths: elixirc_paths(Mix.env()),
      start_permanent: Mix.env() == :prod,
      deps: deps(),
      releases: releases(),
      dialyzer: dialyzer()
    ]
  end

  defp dialyzer do
    [
      plt_file: {:no_warn, "priv/plts/dialyzer.plt"},
      plt_add_apps: [:mix, :ex_unit]
    ]
  end

  defp elixirc_paths(:test), do: ["lib", "test/support"]
  defp elixirc_paths(_), do: ["lib"]

  def application do
    [
      extra_applications: [:logger],
      mod: {McpServerElixir.Application, []}
    ]
  end

  defp deps do
    [
      # Rustler NIF for embedded Core
      {:rustler, "~> 0.36", optional: true},

      # HTTP Client
      {:tesla, "~> 1.11"},
      {:hackney, "~> 4.6"},
      {:jason, "~> 1.4"},

      # WebSocket Client
      {:websockex, "~> 0.4"},

      # PubSub for local event broadcasting
      {:phoenix_pubsub, "~> 2.1"},

      # Broadway for high-throughput event processing
      {:broadway, "~> 1.1"},

      # JSON Schema validation (optional, for input validation)
      {:ex_json_schema, "~> 0.9", optional: true},

      # Telemetry
      {:telemetry, "~> 1.2"},

      # Development & Testing
      {:credo, "~> 1.7", only: [:dev, :test], runtime: false},
      {:dialyxir, "~> 1.4", only: [:dev, :test], runtime: false},
      {:ex_doc, "~> 0.31", only: :dev, runtime: false}
    ]
  end

  defp releases do
    [
      mcp_server_elixir: [
        include_executables_for: [:unix],
        applications: [runtime_tools: :permanent],
        steps: [:assemble, :tar]
      ]
    ]
  end
end
