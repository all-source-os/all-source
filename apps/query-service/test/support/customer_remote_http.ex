defmodule QueryServiceEx.TestSupport.CustomerRemoteHTTP do
  @moduledoc "Synthetic HTTP OAuth fixtures backed by an owned durable Core process."
  import ExUnit.Assertions
  import ExUnit.Callbacks
  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient
  alias QueryServiceEx.TestSupport.CustomerAgentCore

  @tenant "connection-management-test"
  @subject "oauth:google:connection-owner"
  @secret "synthetic-only-remote-http-session-secret-2026"
  @issuer "https://www.example.test"
  @resource @issuer <> "/mcp/customer-review"

  def setup_context do
    context = CustomerAgentCore.setup_context()

    config = [
      customer_connections_enabled: true,
      customer_remote_enabled: true,
      customer_review_enabled: true,
      customer_oauth_issuer: @issuer,
      customer_review_resource: @resource
    ]

    previous = for {key, _} <- config, do: {key, Application.get_env(:query_service_ex, key)}
    for {key, value} <- config, do: Application.put_env(:query_service_ex, key, value)
    previous_secret = System.get_env("JWT_SECRET")
    System.put_env("JWT_SECRET", @secret)
    clear_rates()

    on_exit(fn ->
      for {key, value} <- previous do
        if is_nil(value),
          do: Application.delete_env(:query_service_ex, key),
          else: Application.put_env(:query_service_ex, key, value)
      end

      if previous_secret,
        do: System.put_env("JWT_SECRET", previous_secret),
        else: System.delete_env("JWT_SECRET")

      clear_rates()
    end)

    # test-hang-allow: owned loopback server; requests and supervised shutdown are bounded.
    server =
      start_supervised!({Bandit, plug: QueryServiceExWeb.Endpoint, port: 0, ip: {127, 0, 0, 1}})

    {:ok, {_, port}} = ThousandIsland.listener_info(server)
    Map.put(context, :query_url, "http://127.0.0.1:#{port}")
  end

  def clear_rates do
    for key <- [
          "customer-oauth:admission",
          "customer-remote:admission",
          "customer-connections:admission",
          "customer-connections:" <> @tenant,
          "customer-review:admission",
          "customer-review:" <> @tenant
        ],
        do: :ets.delete(:rate_limiter_buckets, key)
  end

  def request do
    %{
      "client_id" => "claude-ai",
      "redirect_uri" => "https://claude.ai/api/mcp/auth_callback",
      "response_type" => "code",
      "resource" => @resource,
      "scope" => "allsource.review",
      "state" => "synthetic-state-never-render",
      "code_challenge_method" => "S256",
      "code_challenge" => "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    }
  end

  def exchange(code) do
    Map.take(request(), ~w(client_id redirect_uri resource))
    |> Map.merge(%{
      "code" => code,
      "grant_type" => "authorization_code",
      "code_verifier" => "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
    })
  end

  def session(extra \\ %{}) do
    now = System.system_time(:second)

    claims =
      Map.merge(
        %{
          "sub" => @subject,
          "tenant_id" => @tenant,
          "iat" => now,
          "exp" => now + 300,
          "provider" => "google",
          "email_verified" => true,
          "role" => "developer"
        },
        extra
      )

    {_, token} =
      JOSE.JWT.sign(JOSE.JWK.from_oct(@secret), %{"alg" => "HS256"}, claims) |> JOSE.JWS.compact()

    token
  end

  def consent, do: %{"accepted" => true, "version" => "review-metadata-v1"}
  def bearer(token), do: [{"authorization", "Bearer " <> token}]

  def authorize(context) do
    assert {200, %{"request_token" => pending}, _} =
             http(context, :post, "oauth/prepare", request())

    assert {200, %{"code" => code} = issued, _} =
             http(
               context,
               :post,
               "connections/authorize",
               %{"request_token" => pending, "consent" => consent()},
               bearer(session())
             )

    {code, issued}
  end

  def connect(context) do
    {code, _} = authorize(context)
    assert {200, %{"access_token" => token} = issued, _} = token_request(context, exchange(code))
    assert issued["token_type"] == "Bearer"
    assert issued["expires_in"] in 1..3_600
    assert issued["scope"] == "allsource.review"
    refute Map.has_key?(issued, "binding")
    {token, code}
  end

  def token_request(context, fields),
    do:
      http(context, :post, "oauth/token", URI.encode_query(fields), [
        {"content-type", "application/x-www-form-urlencoded"}
      ])

  def http(context, method, path, body \\ nil, headers \\ []) do
    body = if is_map(body), do: Jason.encode!(body), else: body

    headers =
      if List.keymember?(headers, "content-type", 0),
        do: headers,
        else: [{"content-type", "application/json"} | headers]

    client =
      Tesla.client(
        [
          {Tesla.Middleware.BaseUrl, context.query_url},
          {Tesla.Middleware.Timeout, timeout: 10_000}
        ],
        Tesla.Adapter.Hackney
      )

    {:ok, response} =
      Tesla.request(client,
        method: method,
        url: "/api/customer-agent/" <> path,
        headers: headers,
        body: body
      )

    assert {"cache-control", "no-store"} in response.headers
    {response.status, Jason.decode!(response.body), response.headers}
  end

  def provision do
    assert {:ok, %{status: 201}} =
             Tesla.post(RustCoreClient.write_client(), "/api/v1/tenants", %{
               id: @tenant,
               name: "Synthetic remote HTTP workspace"
             })

    members([%{"user_id" => @subject, "role" => "member"}])
    billing("active")
  end

  def members(members) do
    assert {:ok, %{status: 200}} =
             Tesla.post(RustCoreClient.write_client(), "/api/v1/config", %{
               key: "team:#{@tenant}:members",
               value: %{"schema_version" => 2, "members" => members, "invitations" => %{}},
               changed_by: "synthetic-test"
             })
  end

  def billing(status) do
    assert {:ok, %{status: 200}} =
             Tesla.put(RustCoreClient.write_client(), "/api/v1/tenants/#{@tenant}", %{
               metadata: %{
                 "subscription" => %{"tier" => "indie", "status" => status},
                 "quotas" => %{"mcp_scope" => "read", "queries_quota" => 100, "queries_used" => 0}
               }
             })
  end
end
