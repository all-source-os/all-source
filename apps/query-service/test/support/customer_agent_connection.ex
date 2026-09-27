defmodule QueryServiceEx.TestSupport.CustomerAgentConnection do
  @moduledoc false

  def write(context, token, binding) do
    suffix = :crypto.strong_rand_bytes(8) |> Base.encode16(case: :lower)
    directory = Path.join(context.directory, "connection-#{suffix}")
    File.mkdir!(directory)
    File.chmod!(directory, 0o700)
    path = Path.join(directory, "connection.json")
    data = %{version: 1, url: context.query_url, token: token, binding: binding}

    case System.get_env("ALLSOURCE_CUSTOMER_MCP_BINARY") do
      nil ->
        File.write!(path, Jason.encode!(data), [:exclusive])
        File.chmod!(path, 0o600)

      binary ->
        installer = Path.join(Path.dirname(binary), "allsource-customer-connection")
        install(installer, path, Jason.encode!(data))
    end

    path
  end

  defp install(binary, path, data) do
    # test-hang-allow: reader has a 3s process deadline, this owner waits at most 5s.
    port =
      Port.open({:spawn_executable, String.to_charlist(binary)}, [
        :binary,
        :exit_status,
        {:args, ["--install", path]}
      ])

    try do
      Port.command(port, data <> "\n")

      receive do
        {^port, {:exit_status, 0}} -> :ok
        {^port, _} -> raise "Synthetic private connection install failed"
      after
        5_000 -> raise "Synthetic private connection install deadline exceeded"
      end
    after
      if Port.info(port), do: Port.close(port)
    end
  end
end
