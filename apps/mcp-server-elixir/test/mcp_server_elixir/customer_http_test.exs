defmodule McpServerElixir.CustomerHTTPTest do
  use ExUnit.Case, async: false
  alias McpServerElixir.CustomerHTTP

  defmodule QueryFixture do
    @moduledoc false
    import Plug.Conn
    def init(options), do: options

    def call(conn, options) do
      {:ok, _, conn} = read_body(conn, length: 65_536, read_timeout: 1_000)

      status =
        Agent.get_and_update(options[:state], fn
          [head | tail] -> {head, tail}
          [] -> {200, []}
        end)

      conn
      |> put_resp_content_type("application/json")
      |> send_resp(
        status,
        Jason.encode!(%{data: %{state: "connection_verified"}})
      )
    end
  end

  setup do
    state = start_supervised!({Agent, fn -> [] end})

    # test-hang-allow: owned loopback fixtures; every request and supervised shutdown has a deadline.
    query =
      start_supervised!(%{
        id: :query_fixture,
        start:
          {Bandit, :start_link, [[plug: {QueryFixture, state: state}, ip: {127, 0, 0, 1}, port: 0]]}
      })

    {:ok, {_, query_port}} = ThousandIsland.listener_info(query)

    config = [
      customer_review: true,
      customer_review_query_url: "http://127.0.0.1:#{query_port}",
      customer_review_metadata_url: "https://www.example.test/.well-known/oauth-protected-resource",
      customer_review_origins: ["https://www.example.test"]
    ]

    previous = for {key, _} <- config, do: {key, Application.get_env(:mcp_server_elixir, key)}
    for {key, value} <- config, do: Application.put_env(:mcp_server_elixir, key, value)

    on_exit(fn ->
      for {key, value} <- previous do
        if is_nil(value),
          do: Application.delete_env(:mcp_server_elixir, key),
          else: Application.put_env(:mcp_server_elixir, key, value)
      end
    end)

    server =
      start_supervised!(%{
        id: :mcp_fixture,
        start: {Bandit, :start_link, [[plug: CustomerHTTP, ip: {127, 0, 0, 1}, port: 0]]}
      })

    {:ok, {_, port}} = ThousandIsland.listener_info(server)
    %{url: "http://127.0.0.1:#{port}/mcp/customer-review", state: state}
  end

  test "HTTP errors preserve body-read state when authority is lost during tool call", context do
    Agent.update(context.state, fn _ -> [200, 403] end)

    assert %{status: 401, headers: headers} =
             post(context, %{
               jsonrpc: "2.0",
               id: 1,
               method: "tools/call",
               params: %{name: "allsource_review_context", arguments: %{}}
             })

    assert {"cache-control", "no-store"} in headers

    assert Enum.any?(headers, fn {key, value} ->
             key == "www-authenticate" and String.contains?(value, "oauth-protected-resource")
           end)
  end

  test "bad JSON, batches, origin and unknown version fail with HTTP errors", context do
    for body <- ["{", "[]", "{}"] do
      assert %{status: 400} = post(context, body)
    end

    assert %{status: 400} = post(context, %{}, [{"mcp-protocol-version", "unrecognized"}])
    assert %{status: 403} = post(context, %{}, [{"origin", "https://evil.test"}])
    assert %{status: 413} = post(context, String.duplicate("x", 65_537))
    assert %{status: 200} = post(context, %{jsonrpc: "2.0", method: "tools/list", id: 1})

    assert %{status: 202, body: ""} =
             post(context, %{jsonrpc: "2.0", method: "notifications/initialized"})
  end

  test "upstream outage and rate limiting do not become reauthorization loops", context do
    Agent.update(context.state, fn _ -> [503, 429] end)
    assert %{status: 503} = post(context, %{})
    assert %{status: 429} = post(context, %{})
    Application.put_env(:mcp_server_elixir, :customer_review, false)
    assert %{status: 404} = post(context, %{})
  end

  defp post(context, body, headers \\ []) do
    body = if is_map(body), do: Jason.encode!(body), else: body

    headers = [
      {"content-type", "application/json"},
      {"authorization", "Bearer synthetic-envelope"} | headers
    ]

    client = Tesla.client([{Tesla.Middleware.Timeout, timeout: 5_000}], Tesla.Adapter.Hackney)
    assert {:ok, response} = Tesla.post(client, context.url, body, headers: headers)
    response
  end
end
