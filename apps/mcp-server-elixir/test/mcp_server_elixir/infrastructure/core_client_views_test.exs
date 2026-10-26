defmodule McpServerElixir.Infrastructure.CoreClientViewsTest do
  @moduledoc """
  CoreClient against the view routes of docs/contracts/latest-state §4: the
  path and query it sends, and how each documented error reads.
  """
  use ExUnit.Case, async: false

  alias McpServerElixir.Infrastructure.CoreClient

  setup do
    original_url = Application.get_env(:mcp_server_elixir, :core_url)
    on_exit(fn -> Application.put_env(:mcp_server_elixir, :core_url, original_url) end)
    :ok
  end

  # Answers every request with `{status, body}` and sends the requested path
  # (with its query string) to the test process.
  defp start_core(status, body) do
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
    payload = Jason.encode!(body)

    spawn_link(fn -> accept_loop(listen, status, payload, parent) end)
    Application.put_env(:mcp_server_elixir, :core_url, "http://127.0.0.1:#{port}")
    port
  end

  defp accept_loop(listen, status, payload, parent) do
    case :gen_tcp.accept(listen) do
      {:ok, sock} ->
        spawn(fn ->
          with {:ok, data} <- :gen_tcp.recv(sock, 0, 5_000) do
            [request_line | _] = String.split(data, "\r\n")
            [_method, path | _] = String.split(request_line, " ")
            send(parent, {:requested, path})

            :gen_tcp.send(sock, [
              "HTTP/1.1 #{status} X\r\n",
              "content-type: application/json\r\n",
              "content-length: #{byte_size(payload)}\r\n\r\n",
              payload
            ])
          end

          :gen_tcp.close(sock)
        end)

        accept_loop(listen, status, payload, parent)

      _ ->
        :ok
    end
  end

  defp requested do
    assert_received {:requested, path}
    uri = URI.parse(path)
    {uri.path, URI.decode_query(uri.query || "")}
  end

  @row %{
    "view" => "ops_status",
    "key" => "probe-a:2026-10-27",
    "state" => %{"ok" => 1430, "failed" => 2},
    "version" => 57,
    "revision" => 61,
    "generation" => 2,
    "ready" => true
  }

  describe "get_latest_state/2" do
    test "reads one row and returns Core's body" do
      start_core(200, @row)

      assert {:ok, @row} = CoreClient.get_latest_state("ops_status", "probe-a:2026-10-27")
      assert requested() == {"/api/v1/views/ops_status/entities/probe-a%3A2026-10-27", %{}}
    end

    test "percent-encodes a key that contains a slash" do
      start_core(200, @row)

      assert {:ok, _} = CoreClient.get_latest_state("ops_status", "org/a b")
      assert {"/api/v1/views/ops_status/entities/org%2Fa%20b", _} = requested()
    end

    test "a missing row names the key, the view and whether the view is ready" do
      start_core(404, %{"error" => "not_found", "ready" => false})

      assert {:error, message} = CoreClient.get_latest_state("ops_status", "nope")
      assert message =~ ~s("nope")
      assert message =~ ~s("ops_status")
      assert message =~ "ready: false"
    end

    test "a 404 without the not_found body is an unrouted route" do
      start_core(404, %{"error" => "no route"})

      assert {:error, message} = CoreClient.get_latest_state("ops_status", "k")
      assert message =~ "GET /api/v1/views/ops_status/entities/k"
      assert message =~ "does not expose"
    end
  end

  describe "list_latest/2" do
    @page %{
      "view" => "ops_status",
      "rows" => [%{"key" => "probe-a:2026-10-26", "state" => %{}, "version" => 40}],
      "next" => "probe-a:2026-10-26",
      "revision" => 61,
      "content_revision" => 61,
      "generation" => 2,
      "ready" => true
    }

    test "sends prefix, after and limit and returns the page" do
      start_core(200, @page)

      assert {:ok, @page} =
               CoreClient.list_latest("ops_status", %{
                 "prefix" => "probe-a:",
                 "after" => "probe-a:2026-10-25",
                 "limit" => 2
               })

      assert requested() ==
               {"/api/v1/views/ops_status/entities",
                %{"prefix" => "probe-a:", "after" => "probe-a:2026-10-25", "limit" => "2"}}
    end

    test "omits parameters that are not given" do
      start_core(200, @page)

      assert {:ok, _} = CoreClient.list_latest("ops_status", %{"prefix" => nil})
      assert requested() == {"/api/v1/views/ops_status/entities", %{}}
    end

    test "never forwards parameters outside the contract" do
      start_core(200, @page)

      assert {:ok, _} = CoreClient.list_latest("ops_status", %{"tenant_id" => "other"})
      assert requested() == {"/api/v1/views/ops_status/entities", %{}}
    end
  end

  describe "list_changes/2" do
    test "sends after and limit, never wait" do
      start_core(200, %{
        "view" => "ops_status",
        "changes" => [],
        "revision" => 62,
        "caught_up" => true,
        "generation" => 2
      })

      assert {:ok, %{"revision" => 62}} =
               CoreClient.list_changes("ops_status", %{"after" => 61, "limit" => 10, "wait" => 20})

      assert {"/api/v1/views/ops_status/changes", %{"after" => "61", "limit" => "10"} = query} =
               requested()

      refute Map.has_key?(query, "wait")
    end

    test "a view reset tells the caller to re-read the rows" do
      start_core(409, %{"error" => "view_reset", "generation" => 3})

      assert {:error, message} = CoreClient.list_changes("ops_status", %{"after" => 1})
      assert message =~ "generation 3"
      assert message =~ "list_latest"
    end

    test "an unrouted view route is reported, not retried as something else" do
      start_core(405, %{})

      assert {:error, message} = CoreClient.list_changes("ops_status", %{"after" => 1})
      assert message =~ "GET /api/v1/views/ops_status/changes"
      assert message =~ "HTTP 405"
    end
  end
end
