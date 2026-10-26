defmodule McpServerElixir.Infrastructure.CoreClientDegradationTest do
  @moduledoc """
  Regression tests for #229 — every aggregate/state tool failed with a bare
  `-32603` against the hosted gateway while raw event reads worked.

  `GET /api/v1/stats` is a global endpoint the gateway may not route, and an
  entity's state route can 404. Either way the client reports which route
  failed and never rebuilds the answer by pulling the tenant's events: a fold
  over one bounded page of events is wrong for any store larger than the page.
  401/403 surface verbatim too.
  """
  use ExUnit.Case, async: false

  alias McpServerElixir.Infrastructure.CoreClient

  setup do
    original_url = Application.get_env(:mcp_server_elixir, :core_url)
    on_exit(fn -> Application.put_env(:mcp_server_elixir, :core_url, original_url) end)
    :ok
  end

  # A stand-in gateway. `respond` maps the request path (with its query string)
  # to `{status, body}`; every path requested is sent to the test process.
  defp start_gateway(respond) do
    {:ok, listen} =
      :gen_tcp.listen(0, [
        :binary,
        packet: :raw,
        active: false,
        reuseaddr: true,
        ip: {127, 0, 0, 1}
      ])

    {:ok, port} = :inet.port(listen)
    parent = self()

    spawn_link(fn -> accept_loop(listen, respond, parent) end)
    Application.put_env(:mcp_server_elixir, :core_url, "http://127.0.0.1:#{port}")
    port
  end

  defp accept_loop(listen, respond, parent) do
    case :gen_tcp.accept(listen) do
      {:ok, sock} ->
        spawn(fn -> serve(sock, respond, parent) end)
        accept_loop(listen, respond, parent)

      _ ->
        :ok
    end
  end

  defp serve(sock, respond, parent) do
    with {:ok, data} <- :gen_tcp.recv(sock, 0, 5_000) do
      [request_line | _] = String.split(data, "\r\n")
      [_method, path | _] = String.split(request_line, " ")
      send(parent, {:requested, path})

      {code, payload} = respond.(path)
      body = Jason.encode!(payload)

      :gen_tcp.send(sock, [
        "HTTP/1.1 #{code} X\r\n",
        "content-type: application/json\r\n",
        "content-length: #{byte_size(body)}\r\n\r\n",
        body
      ])
    end

    :gen_tcp.close(sock)
  end

  # Events are always served, so a test can prove the client never asked.
  defp gateway_without_route(status) do
    start_gateway(fn path ->
      if String.starts_with?(path, "/api/v1/events/query") do
        {200, %{"events" => [%{"id" => "1", "entity_id" => "e1"}], "count" => 1}}
      else
        {status, %{"error" => "no route"}}
      end
    end)
  end

  describe "get_stats/0 when the gateway does not route /api/v1/stats" do
    for status <- [404, 405] do
      test "#{status} is an error naming the route, with no event download" do
        gateway_without_route(unquote(status))

        assert {:error, message} = CoreClient.get_stats()
        assert message =~ "GET /api/v1/stats"
        assert message =~ "HTTP #{unquote(status)}"
        refute_received {:requested, "/api/v1/events/query" <> _}
      end
    end
  end

  describe "get_stats/0 must not mask credential problems" do
    for status <- [401, 403] do
      test "#{status} surfaces as an error" do
        gateway_without_route(unquote(status))

        assert {:error, message} = CoreClient.get_stats()
        assert message =~ "#{unquote(status)}"
        refute_received {:requested, "/api/v1/events/query" <> _}
      end
    end
  end

  describe "reconstruct_state/2" do
    test "asks Core for the history it drops by default" do
      start_gateway(fn _path ->
        {200, %{"entity_id" => "e1", "current_state" => %{}, "history" => []}}
      end)

      assert {:ok, %{"history" => []}} = CoreClient.reconstruct_state("e1")
      assert_received {:requested, "/api/v1/entities/e1/state?" <> query}
      assert URI.decode_query(query) == %{"history" => "true"}
    end

    test "keeps as_of alongside history" do
      start_gateway(fn _path -> {200, %{"entity_id" => "e1", "current_state" => %{}}} end)

      assert {:ok, _} = CoreClient.reconstruct_state("e1", "2026-07-01T00:00:00Z")
      assert_received {:requested, "/api/v1/entities/e1/state?" <> query}

      assert URI.decode_query(query) == %{"as_of" => "2026-07-01T00:00:00Z", "history" => "true"}
    end

    for status <- [404, 405] do
      test "#{status} is an error naming the route, with no event download" do
        gateway_without_route(unquote(status))

        assert {:error, message} = CoreClient.reconstruct_state("e1")
        assert message =~ "GET /api/v1/entities/e1/state"
        assert message =~ "HTTP #{unquote(status)}"
        refute_received {:requested, "/api/v1/events/query" <> _}
      end
    end
  end

  describe "storage_stats/1" do
    test "reports the stats route failure instead of approximating" do
      gateway_without_route(404)

      assert {:error, message} = CoreClient.storage_stats()
      assert message =~ "GET /api/v1/stats"
      refute_received {:requested, "/api/v1/events/query" <> _}
    end
  end

  describe "exact path: gateway serves tenant-scoped stats (#230)" do
    # Once the gateway routes GET /api/v1/stats tenant-scoped, Core answers with
    # TenantStoreStats. This pins the cross-language shape contract: the field
    # names Rust serialises must be the ones the Elixir tools read. A mismatch
    # here is what made `unique_entities` render 0 against a real response.
    @tenant_stats %{
      "total_events" => 16_432,
      "total_entities" => 87,
      "total_event_types" => 12,
      "total_ingested" => 16_432,
      "event_types" => %{"created" => 9_001, "updated" => 7_431},
      "oldest_event" => "2026-01-02T03:04:05Z",
      "newest_event" => "2026-07-26T01:02:03Z"
    }

    test "returns Core's answer verbatim" do
      stats_gateway()

      assert {:ok, stats} = CoreClient.get_stats()

      assert stats["total_events"] == 16_432
      assert stats["total_entities"] == 87
      assert stats["event_types"] == %{"created" => 9_001, "updated" => 7_431}
      refute Map.has_key?(stats, "source")
      refute Map.has_key?(stats, "approximate")
    end

    test "quick_stats renders the exact entity count from total_entities" do
      stats_gateway()

      state = %{
        backend: CoreClient,
        read_only: false,
        control_plane_enabled: false,
        system_admin: false
      }

      assert {:ok, %{content: [%{text: text} | _]}} =
               McpServerElixir.Protocol.McpTools.call_tool("quick_stats", %{}, state)

      assert text =~ "16432" or text =~ "16,432"
      # 87 entities must appear rather than the 0 the old key mismatch produced.
      assert text =~ "87"
    end
  end

  defp stats_gateway do
    start_gateway(fn path ->
      if String.starts_with?(path, "/api/v1/stats"),
        do: {200, @tenant_stats},
        else: {200, %{"events" => [], "count" => 0}}
    end)
  end
end
