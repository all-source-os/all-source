defmodule McpServerElixir.Infrastructure.CoreClientAuthTest do
  @moduledoc """
  Regression tests for #228 — remote mode against a hosted, authenticated Core.

  Two defects were confirmed:

    1. the HTTP client sent no `Authorization` header at all → a hosted Core
       401s → the MCP layer reports JSON-RPC `-32603 Internal error`
    2. the WebSocket client sent the raw key instead of `Bearer <key>` → same 401

  The issue also reported that `:core_url` was frozen at build time by the
  module-level `plug`. That does NOT reproduce on Tesla 1.18: `plug` arguments are
  compiled into `__middleware__/0` and re-evaluated per request. The
  "core_url resolution" tests below pin that behaviour so a future Tesla upgrade
  or refactor that *does* freeze it fails here instead of in production.
  """
  use ExUnit.Case, async: false

  alias McpServerElixir.Infrastructure.ClientAssertion
  alias McpServerElixir.Infrastructure.CoreClient
  alias McpServerElixir.Infrastructure.CoreCredentials
  alias McpServerElixir.Infrastructure.CoreWebSocketClient

  @fixtures Path.expand("../../../../../sdks/rust/tests/fixtures", __DIR__)

  setup do
    original_url = Application.get_env(:mcp_server_elixir, :core_url)
    original_key = Application.get_env(:mcp_server_elixir, :core_api_key)
    original_assertion = Application.get_env(:mcp_server_elixir, :client_assertion)

    on_exit(fn ->
      restore(:core_url, original_url)
      restore(:core_api_key, original_key)
      restore(:client_assertion, original_assertion)
    end)

    Application.delete_env(:mcp_server_elixir, :client_assertion)
    :ok
  end

  defp restore(key, nil), do: Application.delete_env(:mcp_server_elixir, key)
  defp restore(key, value), do: Application.put_env(:mcp_server_elixir, key, value)

  # Pull the values back out of the middleware stack Tesla builds for a request.
  defp base_url do
    Enum.find_value(CoreClient.__middleware__(), fn
      {Tesla.Middleware.BaseUrl, :call, [url]} -> url
      _ -> nil
    end)
  end

  # What CoreCredentials puts on a request as it passes through the middleware.
  defp auth_header do
    {:ok, env} = CoreCredentials.call(%Tesla.Env{}, [], [])
    Tesla.get_header(env, "authorization")
  end

  describe "core_url resolution (reported as build-time-frozen; pinned here)" do
    test "base URL is read at call time, not frozen at compile time" do
      Application.put_env(:mcp_server_elixir, :core_url, "https://api.all-source.xyz")
      assert base_url() == "https://api.all-source.xyz"

      # Same module, same beam — a second value must be picked up, which a
      # compile-time `plug` argument could not do.
      Application.put_env(:mcp_server_elixir, :core_url, "http://other-core:3900")
      assert base_url() == "http://other-core:3900"
    end

    test "falls back to the localhost default when unconfigured" do
      Application.delete_env(:mcp_server_elixir, :core_url)
      assert base_url() == "http://localhost:3900"
    end
  end

  describe "HTTP Authorization header (confirmed bug 1: no auth middleware)" do
    test "attaches the API key as a Bearer token" do
      Application.put_env(:mcp_server_elixir, :core_api_key, "as_live_abc123")

      assert auth_header() == "Bearer as_live_abc123"
    end

    test "does not double-prefix a key that already carries the scheme" do
      Application.put_env(:mcp_server_elixir, :core_api_key, "Bearer as_live_abc123")

      assert auth_header() == "Bearer as_live_abc123"
    end

    test "sends no Authorization header when no key is configured" do
      Application.delete_env(:mcp_server_elixir, :core_api_key)
      assert CoreClient.auth_headers() == []
      assert auth_header() == nil
    end

    test "treats an empty key as unset" do
      Application.put_env(:mcp_server_elixir, :core_api_key, "")
      assert CoreClient.auth_headers() == []
      assert auth_header() == nil
    end

    test "the key is picked up at call time" do
      Application.put_env(:mcp_server_elixir, :core_api_key, "key-one")
      assert auth_header() == "Bearer key-one"

      Application.put_env(:mcp_server_elixir, :core_api_key, "key-two")
      assert auth_header() == "Bearer key-two"
    end

    test "the credential is applied inside Retry, so every attempt asks for it again" do
      stack = Enum.map(CoreClient.__middleware__(), fn {module, _fun, _args} -> module end)
      refute Tesla.Middleware.Headers in stack

      retry = Enum.find_index(stack, &(&1 == Tesla.Middleware.Retry))
      credentials = Enum.find_index(stack, &(&1 == CoreCredentials))
      assert retry && credentials && credentials > retry
    end
  end

  describe "ES256 client assertion (ALLSOURCE_CLIENT_SIGNING_KEY set)" do
    setup do
      settings = [
        signing_key: Path.join(@fixtures, "es256_interop_test_key.pem"),
        client_id: "longhand-control-plane",
        tenant_id: "asserted-tenant",
        kid: "q_OqpJGBOYfCf5q-_NvXB8sEV7Zo9CN_EUHRewTCSSQ",
        audience: "https://api.all-source.xyz"
      ]

      start_supervised!({ClientAssertion, settings})
      Application.put_env(:mcp_server_elixir, :client_assertion, settings)
      Application.put_env(:mcp_server_elixir, :core_api_key, "as_live_static")
      :ok
    end

    test "HTTP sends the assertion and ignores the static key" do
      "Bearer " <> token = auth_header()
      assert [_, _, _] = String.split(token, ".")
      refute token =~ "as_live_static"

      [header | _] = String.split(token, ".")
      decoded = header |> Base.url_decode64!(padding: false) |> Jason.decode!()
      assert decoded["kid"] == "q_OqpJGBOYfCf5q-_NvXB8sEV7Zo9CN_EUHRewTCSSQ"
      assert decoded["alg"] == "ES256"
      assert CoreClient.auth_headers() == [{"authorization", "Bearer " <> token}]
    end

    test "the WebSocket sends the same cached assertion" do
      "Bearer " <> token = auth_header()
      assert CoreWebSocketClient.build_auth_headers() == [{"Authorization", "Bearer " <> token}]
    end

    test "a reconnect replaces the Authorization header and keeps the others" do
      conn = %WebSockex.Conn{
        extra_headers: [{"Authorization", "Bearer stale"}, {"x-trace", "1"}]
      }

      refreshed = CoreWebSocketClient.refresh_auth(conn)
      assert {"x-trace", "1"} in refreshed.extra_headers
      assert [{"Authorization", "Bearer " <> token}] = refreshed.extra_headers -- [{"x-trace", "1"}]
      refute token == "stale"
    end
  end

  describe "WebSocket Authorization header (confirmed bug 2: raw key, no scheme)" do
    test "sends the key with the Bearer scheme" do
      Application.put_env(:mcp_server_elixir, :core_api_key, "as_live_abc123")

      assert CoreWebSocketClient.build_auth_headers() == [
               {"Authorization", "Bearer as_live_abc123"}
             ]
    end

    test "does not double-prefix an already-scheme'd key" do
      Application.put_env(:mcp_server_elixir, :core_api_key, "Bearer as_live_abc123")

      assert CoreWebSocketClient.build_auth_headers() == [
               {"Authorization", "Bearer as_live_abc123"}
             ]
    end

    test "sends no header when the key is unset or empty" do
      Application.delete_env(:mcp_server_elixir, :core_api_key)
      assert CoreWebSocketClient.build_auth_headers() == []

      Application.put_env(:mcp_server_elixir, :core_api_key, "")
      assert CoreWebSocketClient.build_auth_headers() == []
    end
  end
end
