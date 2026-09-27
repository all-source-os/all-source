defmodule QueryServiceEx.TestSupport.CustomerRemoteMCP do
  @moduledoc "Runs the separately built customer MCP HTTP release; never imports another app."
  import ExUnit.Assertions

  def with_mcp(context, fun, http_port \\ nil) do
    http_port = http_port || free_port()
    url = "http://127.0.0.1:#{http_port}"

    env =
      %{
        "ALLSOURCE_CUSTOMER_REVIEW" => "true",
        "ALLSOURCE_CUSTOMER_REVIEW_HTTP" => "true",
        "CUSTOMER_REVIEW_QUERY_URL" => context.query_url,
        "CUSTOMER_REVIEW_HTTP_PORT" => to_string(http_port),
        "CUSTOMER_REVIEW_HTTP_IP" => "127.0.0.1",
        "CUSTOMER_REVIEW_ORIGINS" => "https://www.example.test",
        "CUSTOMER_REVIEW_METADATA_URL" =>
          "https://www.example.test/.well-known/oauth-protected-resource",
        "CORE_API_KEY" => "",
        "ALLSOURCE_CORE_API_KEY" => "",
        "CORE_MODE" => "remote",
        "CORE_WS_ENABLED" => "false",
        "ALLSOURCE_SYSTEM_ADMIN" => "true"
      }
      |> Enum.map(fn {key, value} -> {String.to_charlist(key), String.to_charlist(value)} end)

    # test-hang-allow: owned release, bounded readiness; always SIGKILL and await its exit.
    port =
      Port.open(
        {:spawn_executable,
         String.to_charlist(System.fetch_env!("ALLSOURCE_CUSTOMER_MCP_BINARY"))},
        [:binary, :exit_status, :stderr_to_stdout, {:args, ["start"]}, {:env, env}]
      )

    try do
      ready(port, url, 40)
      fun.(url)
    after
      case Port.info(port, :os_pid) do
        {:os_pid, pid} ->
          System.cmd("/bin/kill", ["-KILL", to_string(pid)], stderr_to_stdout: true)
          stop(port, System.monotonic_time(:millisecond) + 5_000)

        nil ->
          :ok
      end
    end
  end

  def http(url, method, token, message \\ nil, headers \\ [], query \\ "") do
    headers = [
      {"content-type", "application/json"},
      {"accept", "application/json, text/event-stream"},
      {"mcp-protocol-version", "2025-11-25"} | headers
    ]

    headers = if token, do: [{"authorization", "Bearer " <> token} | headers], else: headers
    body = if is_map(message), do: Jason.encode!(message), else: message
    client = Tesla.client([{Tesla.Middleware.Timeout, timeout: 10_000}], Tesla.Adapter.Hackney)

    {:ok, response} =
      Tesla.request(client,
        method: method,
        url: url <> "/mcp/customer-review" <> query,
        body: body,
        headers: headers
      )

    data = if response.body == "", do: nil, else: Jason.decode!(response.body)
    {response.status, data, response.headers}
  end

  defp ready(_port, _url, 0), do: flunk("Owned MCP HTTP process did not become ready")

  defp ready(port, url, remaining) do
    drain(port, 100)
    client = Tesla.client([{Tesla.Middleware.Timeout, timeout: 300}], Tesla.Adapter.Hackney)

    case Tesla.get(client, url <> "/mcp/customer-review") do
      {:ok, %{status: 401}} ->
        :ok

      _ ->
        # test-hang-allow: at most 40 loopback readiness probes.
        Process.sleep(75)
        ready(port, url, remaining - 1)
    end
  end

  defp drain(_port, 0), do: :ok

  defp drain(port, remaining) do
    receive do
      {^port, {:data, _}} -> drain(port, remaining - 1)
      {^port, {:exit_status, _}} -> flunk("Owned MCP HTTP process exited before readiness")
    after
      0 -> :ok
    end
  end

  defp stop(port, deadline) do
    remaining = deadline - System.monotonic_time(:millisecond)
    if remaining <= 0, do: flunk("Owned MCP HTTP process did not exit")

    receive do
      {^port, {:exit_status, _}} -> :ok
      {^port, {:data, _}} -> stop(port, deadline)
    after
      remaining -> flunk("Owned MCP HTTP process did not exit")
    end
  end

  defp free_port do
    # test-hang-allow: reserve an ephemeral loopback port and immediately close it.
    {:ok, socket} = :gen_tcp.listen(0, [:binary, active: false, ip: {127, 0, 0, 1}])
    {:ok, port} = :inet.port(socket)
    :ok = :gen_tcp.close(socket)
    port
  end
end
