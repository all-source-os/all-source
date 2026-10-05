defmodule Mix.Tasks.Compile.CustomerConnection do
  use Mix.Task.Compiler

  @moduledoc false
  @recursive true

  def run(_args) do
    root = Path.join(__DIR__, "tooling/customer-connection")
    cargo = System.find_executable("cargo") || Mix.raise("Rust cargo is required")

    {_output, status} =
      System.cmd(
        cargo,
        ["build", "--release", "--locked", "--manifest-path", root <> "/Cargo.toml"],
        into: IO.stream(:stdio, :line),
        stderr_to_stdout: true,
        env: [{"CARGO_TARGET_DIR", root <> "/target"}]
      )

    if status != 0, do: Mix.raise("Customer connection reader build failed")
    destination = Path.join(__DIR__, "priv/bin")
    File.mkdir_p!(destination)

    File.cp!(
      root <> "/target/release/allsource-customer-connection",
      destination <> "/allsource-customer-connection"
    )

    File.chmod!(destination <> "/allsource-customer-connection", 0o755)
    {:ok, []}
  end
end

defmodule McpServerElixir.MixProject do
  use Mix.Project

  def project do
    [
      app: :mcp_server_elixir,
      version: "0.25.3",
      elixir: "~> 1.17",
      compilers: [:customer_connection] ++ Mix.compilers(),
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
      {:bandit, "~> 1.12"},

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
        steps: [:assemble, &install_connection_command/1, :tar]
      ]
    ]
  end

  defp install_connection_command(release) do
    destination = Path.join(release.path, "bin/allsource-customer-connection")
    File.cp!(Path.join(__DIR__, "priv/bin/allsource-customer-connection"), destination)
    File.chmod!(destination, 0o755)
    release
  end
end
