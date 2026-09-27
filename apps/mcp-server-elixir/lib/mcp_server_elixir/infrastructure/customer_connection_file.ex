defmodule McpServerElixir.Infrastructure.CustomerConnectionFile do
  @moduledoc """
  Loads a connection through the release's OS-owner-checking reader on each call.
  A path may be supplied by the host; credential contents never come from host
  environment variables or tool arguments. Local ownership is not host attestation.
  """

  @max_bytes 8_192
  @deadline_ms 4_000

  @spec load() :: {:ok, map()} | {:error, :invalid_connection}
  def load do
    path = Application.get_env(:mcp_server_elixir, :customer_review_connection_file)

    with true <- is_binary(path) and byte_size(path) in 1..4_096,
         {:ok, raw} <- read(path),
         {:ok, config} <- Jason.decode(raw),
         true <- valid?(config) do
      {:ok, config}
    else
      _ -> {:error, :invalid_connection}
    end
  rescue
    _ -> {:error, :invalid_connection}
  end

  defp read(path) do
    executable =
      :mcp_server_elixir
      |> :code.priv_dir()
      |> Path.join("bin/allsource-customer-connection")
      |> String.to_charlist()

    port =
      Port.open({:spawn_executable, executable}, [
        :binary,
        :exit_status,
        :hide,
        {:args, [path]}
      ])

    deadline = System.monotonic_time(:millisecond) + @deadline_ms

    try do
      receive_bytes(port, "", deadline)
    after
      if Port.info(port), do: Port.close(port)
    end
  end

  defp receive_bytes(port, bytes, deadline) do
    remaining = max(deadline - System.monotonic_time(:millisecond), 0)

    receive do
      {^port, {:data, data}} when byte_size(bytes) + byte_size(data) <= @max_bytes ->
        receive_bytes(port, bytes <> data, deadline)

      {^port, {:exit_status, 0}} when byte_size(bytes) > 0 ->
        {:ok, bytes}

      {^port, _} ->
        {:error, :invalid_connection}
    after
      remaining -> {:error, :invalid_connection}
    end
  end

  defp valid?(%{"version" => 1, "url" => url, "token" => token, "binding" => binding} = config)
       when map_size(config) == 4 and is_binary(token) and byte_size(token) == 109 do
    Regex.match?(~r/\Aasreview_v2_[0-9a-f]{32}\.[0-9a-f]{64}\z/, token) and
      valid_binding?(binding) and valid_destination?(url, binding["resource"])
  end

  defp valid?(_), do: false

  defp valid_binding?(
         %{
           "tenant_id" => tenant,
           "subject_id" => subject,
           "client_id" => "claude-code",
           "resource" => resource
         } = binding
       )
       when map_size(binding) == 4 do
    bounded_match?(tenant, 128, ~r/\A[A-Za-z0-9_-]+\z/) and
      bounded_match?(subject, 256, ~r/\A[A-Za-z0-9_:@.+-]+\z/) and
      is_binary(resource) and byte_size(resource) in 1..512
  end

  defp valid_binding?(_), do: false

  defp bounded_match?(value, limit, pattern),
    do: is_binary(value) and byte_size(value) in 1..limit and Regex.match?(pattern, value)

  defp valid_destination?(url, resource) when is_binary(url) and byte_size(url) in 1..512 do
    with {:ok, endpoint} <- URI.new(url),
         {:ok, audience} <- URI.new(resource),
         true <- safe_uri?(endpoint) and safe_uri?(audience),
         true <- endpoint.path in [nil, "", "/"],
         true <- audience.scheme == "https" do
      same_origin =
        endpoint.scheme == audience.scheme and endpoint.host == audience.host and
          endpoint.port == audience.port

      # Local development can address a loopback QS whose configured audience is
      # HTTPS. Non-loopback credentials can only go to their audience's origin.
      same_origin or (endpoint.scheme == "http" and endpoint.host in ["127.0.0.1", "::1"])
    else
      _ -> false
    end
  end

  defp valid_destination?(_, _), do: false

  defp safe_uri?(%URI{host: host, userinfo: nil, query: nil, fragment: nil}),
    do: is_binary(host) and host != ""

  defp safe_uri?(_), do: false
end
