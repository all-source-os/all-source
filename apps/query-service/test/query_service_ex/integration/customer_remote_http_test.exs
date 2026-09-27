defmodule QueryServiceEx.Integration.CustomerRemoteHTTPTest do
  use ExUnit.Case, async: false
  alias QueryServiceEx.Infrastructure.Adapters.CustomerRemoteTokens
  alias QueryServiceEx.TestSupport.CustomerRemoteHTTP, as: Remote
  import QueryServiceEx.TestSupport.CustomerAgentCore, only: [with_core: 2]
  @moduletag :integration
  @moduletag timeout: 60_000
  @moduletag skip: is_nil(System.get_env("ALLSOURCE_CORE_BINARY"))

  setup do
    Remote.setup_context()
  end

  test "remote admission rate limits even invalid credentials before decryption", context do
    :ets.insert(
      :rate_limiter_buckets,
      {"customer-remote:admission", -1_000, System.monotonic_time(:millisecond)}
    )

    assert {429, %{"error" => %{"code" => "rate_limited"}}, headers} =
             Remote.http(context, :post, "remote/context", %{}, Remote.bearer("invalid"))

    assert {"retry-after", "1"} in headers
  end

  test "discovery and strict public form exchange expose only encrypted, bound credentials",
       context do
    with_core(context, fn ->
      Remote.provision()
      assert {200, metadata, _} = Remote.http(context, :get, "oauth/metadata")
      assert metadata["issuer"] == "https://www.example.test"
      assert metadata["code_challenge_methods_supported"] == ["S256"]
      assert metadata["token_endpoint_auth_methods_supported"] == ["none"]
      refute Map.has_key?(metadata, "registration_endpoint")
      assert {200, resource, _} = Remote.http(context, :get, "oauth/resource")
      assert resource["resource"] == Remote.request()["resource"]
      {token, code} = Remote.connect(context)

      assert {200, %{"data" => %{"state" => "eligibility_verified"}}, _} =
               Remote.http(context, :post, "remote/context", %{}, Remote.bearer(token))

      assert {:ok, payload} =
               CustomerRemoteTokens.open_access(
                 token,
                 resource["resource"],
                 System.system_time(:second)
               )

      refute token =~ payload["token"]

      for invalid <- [payload["token"], Remote.session(), code] do
        assert {401, _, _} =
                 Remote.http(context, :post, "remote/context", %{}, Remote.bearer(invalid))
      end

      assert {401, _, _} =
               Remote.http(
                 context,
                 :post,
                 "remote/context",
                 %{"binding" => payload["binding"]},
                 Remote.bearer(token)
               )

      assert {400, %{"error" => "invalid_grant"}, _} =
               Remote.token_request(context, Remote.exchange(code))

      assert {403, _, _} =
               Remote.http(context, :post, "remote/context", %{}, Remote.bearer(token))

      {reconnected, _} = Remote.connect(context)

      assert {200, _, _} =
               Remote.http(context, :post, "remote/context", %{}, Remote.bearer(reconnected))

      Remote.members([])

      assert {403, _, _} =
               Remote.http(context, :post, "remote/context", %{}, Remote.bearer(reconnected))
    end)
  end

  test "human authority and explicit consent are required, then revocation stays live", context do
    with_core(context, fn ->
      Remote.provision()

      assert {200, %{"request_token" => pending}, _} =
               Remote.http(context, :post, "oauth/prepare", Remote.request())

      body = %{"request_token" => pending, "consent" => Remote.consent()}

      for extra <- [
            %{"email_verified" => false},
            %{"is_demo" => true},
            %{"is_api_key" => true},
            %{"tenant_id" => "other"},
            %{"sub" => "oauth:google:outsider"},
            %{"exp" => 1}
          ] do
        assert {403, _, _} =
                 Remote.http(
                   context,
                   :post,
                   "connections/authorize",
                   body,
                   Remote.bearer(Remote.session(extra))
                 )
      end

      assert {403, _, _} =
               Remote.http(
                 context,
                 :post,
                 "connections/authorize",
                 Map.put(body, "consent", %{}),
                 Remote.bearer(Remote.session())
               )

      {token, _} = Remote.connect(context)

      assert {200, %{"data" => %{"connections" => [receipt]}}, _} =
               Remote.http(
                 context,
                 :post,
                 "connections/list",
                 %{},
                 Remote.bearer(Remote.session())
               )

      Remote.billing("canceled")

      assert {403, _, _} =
               Remote.http(context, :post, "remote/context", %{}, Remote.bearer(token))

      assert {200, _, _} =
               Remote.http(
                 context,
                 :post,
                 "connections/revoke",
                 %{"id" => receipt["id"]},
                 Remote.bearer(Remote.session())
               )

      Remote.billing("active")

      assert {403, _, _} =
               Remote.http(context, :post, "remote/context", %{}, Remote.bearer(token))
    end)
  end

  test "duplicate form fields, malformed encoding, substitution, query data and oversized bodies deny without consuming code",
       context do
    with_core(context, fn ->
      Remote.provision()
      {code, _} = Remote.authorize(context)
      form = URI.encode_query(Remote.exchange(code))

      for body <- [form <> "&client_id=claude-ai", form <> "&bad=%", form <> "&bad=%FF"] do
        assert {400, %{"error" => "invalid_request"}, _} =
                 Remote.http(context, :post, "oauth/token", body, [
                   {"content-type", "application/x-www-form-urlencoded"}
                 ])
      end

      for {key, value} <- [
            {"client_id", "other"},
            {"resource", "https://other.test/mcp"},
            {"code_verifier", String.duplicate("x", 43)}
          ] do
        assert {400, %{"error" => "invalid_grant"}, _} =
                 Remote.token_request(context, Map.put(Remote.exchange(code), key, value))
      end

      assert {415, _, _} = Remote.http(context, :post, "oauth/token", Remote.exchange(code))
      assert {403, _, _} = Remote.http(context, :post, "oauth/token?code=private", form)

      assert {413, _, _} =
               Remote.http(context, :post, "oauth/token", String.duplicate("x", 8_193), [
                 {"content-type", "application/x-www-form-urlencoded"}
               ])

      assert {400, _, _} =
               Remote.http(context, :post, "oauth/token", form, [
                 {"content-type", "application/x-www-form-urlencoded"}
                 | Remote.bearer(Remote.session())
               ])

      assert {200, _, _} = Remote.token_request(context, Remote.exchange(code))
      Application.put_env(:query_service_ex, :customer_remote_enabled, false)
      assert {404, _, _} = Remote.http(context, :get, "oauth/metadata")
    end)
  end
end
