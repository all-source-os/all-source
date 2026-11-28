defmodule McpServerElixir.EventHistoryHTTPTest do
  use ExUnit.Case, async: false
  import Plug.Conn
  import Plug.Test
  alias McpServerElixir.CustomerHTTP

  @path "/mcp/event-history"
  @session "22675b42-9b9e-4a98-a8ab-e613703fd26a"
  @origin "https://history.example.test"

  setup do
    for {key, value} <- [event_history: true, event_history_origin: @origin] do
      old = Application.fetch_env(:mcp_server_elixir, key)
      Application.put_env(:mcp_server_elixir, key, value)

      on_exit(fn ->
        case old do
          {:ok, v} -> Application.put_env(:mcp_server_elixir, key, v)
          :error -> Application.delete_env(:mcp_server_elixir, key)
        end
      end)
    end

    :ok
  end

  test "unauthenticated host discovers the history audience" do
    res = CustomerHTTP.call(conn(:post, @path), [])
    assert res.status == 401
    assert [challenge] = get_resp_header(res, "www-authenticate")
    assert challenge =~ "/.well-known/oauth-protected-resource/mcp/event-history"
    assert challenge =~ "allsource.history.read"
  end

  test "history listener keeps the separately gated review endpoint closed" do
    old = Application.fetch_env(:mcp_server_elixir, :customer_review)
    Application.put_env(:mcp_server_elixir, :customer_review, true)

    on_exit(fn ->
      case old do
        {:ok, v} -> Application.put_env(:mcp_server_elixir, :customer_review, v)
        :error -> Application.delete_env(:mcp_server_elixir, :customer_review)
      end
    end)

    assert CustomerHTTP.call(conn(:post, "/mcp/customer-review"), []).status == 404
  end

  test "initialized host lists one tool and forwards a logical checkout request" do
    timeline = %{
      "workspace" => %{"id" => "engineering", "name" => "Engineering"},
      "entity" => "checkout",
      "from" => "2026-11-25T10:00:00Z",
      "to" => "2026-11-26T10:00:00Z",
      "coverage" => "complete",
      "events" => []
    }

    caller = fn
      "session", args, "synthetic" ->
        assert args in [%{}, %{"session_id" => @session}]
        {:ok, %{"session_id" => @session}}

      "timeline", args, "synthetic" ->
        assert args == %{
                 "session_id" => @session,
                 "request_id" => 7,
                 "arguments" => %{"entity" => "checkout"}
               }

        {:ok, timeline}
    end

    init = post("initialize", %{"protocolVersion" => "2025-11-25"}, caller, nil)
    assert [@session] == get_resp_header(init, "mcp-session-id")

    assert %{"result" => %{"tools" => [%{"name" => "get_event_history"}]}} =
             Jason.decode!(post("tools/list", %{}, caller).resp_body)

    result =
      post(
        "tools/call",
        %{"name" => "get_event_history", "arguments" => %{"entity" => "checkout"}},
        caller
      )

    assert %{"result" => %{"structuredContent" => ^timeline}} = Jason.decode!(result.resp_body)
    assert post("tools/list", %{}, caller, nil).status == 400
    refused = post("tools/call", %{"name" => "delete", "arguments" => %{}}, caller)
    assert %{"error" => %{"code" => -32602}} = Jason.decode!(refused.resp_body)
  end

  defp post(method, params, caller, session \\ @session) do
    body = Jason.encode!(%{jsonrpc: "2.0", method: method, params: params, id: 7})

    conn =
      conn(:post, @path, body)
      |> put_req_header("content-type", "application/json")
      |> put_req_header("accept", "application/json, text/event-stream")
      |> put_req_header("authorization", "Bearer synthetic")

    conn = if session, do: put_req_header(conn, "mcp-session-id", session), else: conn
    McpServerElixir.EventHistoryHTTP.call(conn, caller)
  end
end
