defmodule QueryServiceEx.Integration.CustomerConnectionsTest do
  use ExUnit.Case, async: false
  import QueryServiceEx.TestSupport.CustomerAgentCore, only: [with_core: 2]
  alias QueryServiceEx.Infrastructure.Adapters.CustomerAgentGrantStore
  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient
  alias QueryServiceEx.TestSupport.CustomerAgentCore

  @moduletag :integration
  @moduletag timeout: 60_000
  @moduletag skip: is_nil(System.get_env("ALLSOURCE_CORE_BINARY"))
  @secret "synthetic-customer-connection-human-session-secret"
  @tenant "connection-management-test"
  @subject "oauth:google:connection-owner"
  @resource "https://api.example.test/customer-review"
  @consent %{"accepted" => true, "version" => "review-metadata-v1"}
  @params %{
    "client_id" => "claude-code",
    "operations" => ["read_context"],
    "ttl" => 300,
    "consent" => @consent
  }

  defmodule BrowserFixture do
    @moduledoc false
    import Plug.Conn
    def init(options), do: options

    def call(conn, options) do
      case {conn.method, conn.request_path} do
        {"POST", "/api/v1/auth/login"} ->
          {:ok, body, conn} = read_body(conn, length: 4096, read_timeout: 1000)

          if Jason.decode!(body)["email"] == "connections@example.test",
            do: respond(conn, %{token: options[:token], new_user: false}),
            else: send_resp(conn, 403, "denied")

        {"GET", path} when path in ["/api/auth/me", "/api/v1/auth/me"] ->
          respond(conn, %{
            data: %{
              user: %{
                id: "oauth:google:connection-owner",
                name: "Synthetic owner",
                email: "connections@example.test",
                role: "developer",
                provider: "google"
              }
            }
          })

        {"GET", "/api/tenant"} ->
          respond(conn, %{
            data: %{
              id: "connection-management-test",
              name: "Synthetic connection workspace",
              subscription_tier: "indie",
              quotas: %{}
            }
          })

        {"POST", "/fixture/stop"} ->
          send(options[:owner], :fixture_stop)
          send_resp(conn, 204, "")

        _ ->
          QueryServiceExWeb.Endpoint.call(conn, QueryServiceExWeb.Endpoint.init([]))
      end
    end

    defp respond(conn, data),
      do: conn |> put_resp_content_type("application/json") |> send_resp(200, Jason.encode!(data))
  end

  setup do
    context = CustomerAgentCore.setup_context()

    previous =
      for key <- [:customer_connections_enabled, :customer_review_resource],
          do: {key, Application.get_env(:query_service_ex, key)}

    Application.put_env(:query_service_ex, :customer_connections_enabled, true)
    Application.put_env(:query_service_ex, :customer_review_resource, @resource)
    previous_secret = System.get_env("JWT_SECRET")
    System.put_env("JWT_SECRET", @secret)

    on_exit(fn ->
      for {key, value} <- previous do
        if is_nil(value),
          do: Application.delete_env(:query_service_ex, key),
          else: Application.put_env(:query_service_ex, key, value)
      end

      if previous_secret,
        do: System.put_env("JWT_SECRET", previous_secret),
        else: System.delete_env("JWT_SECRET")

      :ets.delete(:rate_limiter_buckets, "customer-connections:admission")
      :ets.delete(:rate_limiter_buckets, "customer-connections:" <> @tenant)
    end)

    # test-hang-allow: owned HTTP fixture; every request and child has a deadline.
    server =
      start_supervised!({Bandit, plug: QueryServiceExWeb.Endpoint, port: 0, ip: {127, 0, 0, 1}})

    {:ok, {_address, port}} = ThousandIsland.listener_info(server)
    Map.put(context, :query_url, "http://127.0.0.1:#{port}")
  end

  test "signed human creates consent-bound grant, lists privately, and revokes after billing expires",
       context do
    with_core(context, fn ->
      provision()

      assert {400, _} =
               request(context, "create", session(), Map.put(@params, "tenant_id", "other"))

      assert {400, _} = request(context, "create", session(), Map.put(@params, "consent", %{}))

      assert {400, _} =
               request(context, "create", session(), Map.put(@params, "client_id", "claude-ai"))

      assert {413, _} =
               request(
                 context,
                 "create",
                 session(),
                 Map.put(@params, "padding", String.duplicate("x", 4096))
               )

      assert {200, %{"data" => issued}} = request(context, "create", session(), @params)
      assert issued["binding"] == grant_binding()

      assert {:ok, grant} =
               CustomerAgentGrantStore.verify_credential(
                 issued["token"],
                 grant_binding(),
                 "read_context",
                 System.system_time(:second)
               )

      assert grant["consent"]["version"] == "review-metadata-v1"

      assert {200, %{"data" => %{"connections" => [receipt]}}} =
               request(context, "list", session(), %{})

      refute Jason.encode!(receipt) =~ issued["token"]
      refute Map.has_key?(receipt, "token_hash")

      assert {403, _} =
               request(context, "revoke", session(%{"sub" => "oauth:google:outsider"}), %{
                 "id" => issued["id"]
               })

      set_metadata(%{
        "subscription" => %{"tier" => "indie", "status" => "canceled"},
        "quotas" => %{}
      })

      assert {403, _} = request(context, "create", session(), @params)

      assert {200, %{"data" => %{"revoked" => true}}} =
               request(context, "revoke", session(), %{"id" => issued["id"]})

      assert {:error, :unauthorized} =
               CustomerAgentGrantStore.verify_credential(
                 issued["token"],
                 grant_binding(),
                 "read_context",
                 System.system_time(:second)
               )

      assert {200, %{"data" => %{"connections" => [%{"status" => "revoked"}]}}} =
               request(context, "list", session(), %{})
    end)
  end

  test "agent credentials, demo, impersonation, unverified and expired sessions cannot manage connections",
       context do
    with_core(context, fn ->
      provision()

      {:ok, issued} =
        CustomerAgentGrantStore.issue(
          grant_binding(),
          ["read_context"],
          @consent,
          System.system_time(:second),
          300
        )

      for token <- [
            issued.token,
            CustomerAgentCore.token("admin"),
            session(%{"provider" => nil}),
            session(%{"email_verified" => false}),
            session(%{"is_api_key" => true}),
            session(%{"is_demo" => true}),
            session(%{"view_as" => true}),
            session(%{"core_api_key" => "synthetic-private"}),
            session(%{"exp" => 1}),
            session(%{"nbf" => System.system_time(:second) + 300})
          ] do
        assert {403, _} = request(context, "create", token, @params)
      end

      set_members([])
      assert {403, _} = request(context, "create", session(), @params)
      assert {403, _} = request(context, "list", session(), %{})
      Application.put_env(:query_service_ex, :customer_connections_enabled, false)
      assert {403, _} = request(context, "create", session(), @params)
    end)
  end

  test "actual Core conditional writes enforce the live limit across concurrent clients and restart",
       context do
    now = System.system_time(:second)

    with_core(context, fn ->
      provision()

      for _ <- 1..15,
          do:
            assert(
              {:ok, _} =
                CustomerAgentGrantStore.issue(
                  grant_binding(),
                  ["read_context"],
                  @consent,
                  now,
                  300
                )
            )

      results =
        1..8
        |> Task.async_stream(
          fn _ ->
            CustomerAgentGrantStore.issue(grant_binding(), ["read_context"], @consent, now, 300)
          end,
          timeout: 20_000,
          max_concurrency: 8
        )
        |> Enum.map(fn {:ok, value} -> value end)

      assert Enum.count(results, &match?({:ok, _}, &1)) == 1
      assert {:ok, records} = CustomerAgentGrantStore.list(@tenant, @subject, now)
      assert length(records) == 16
    end)

    with_core(context, fn ->
      assert {:error, :connection_limit} =
               CustomerAgentGrantStore.issue(
                 grant_binding(),
                 ["read_context"],
                 @consent,
                 now,
                 300
               )

      assert {:ok, records} = CustomerAgentGrantStore.list(@tenant, @subject, now)
      assert length(records) == 16
    end)
  end

  @tag :browser_fixture
  @tag timeout: 280_000
  @tag skip: System.get_env("ALLSOURCE_CONNECTION_BROWSER_FIXTURE") != "1"
  test "opt-in browser fixture with actual connection handlers and Core", context do
    with_core(context, fn ->
      provision()
      # test-hang-allow: manual fixture has 240s deadline and a local stop route.
      start_supervised!(%{
        id: BrowserFixture,
        start:
          {Bandit, :start_link,
           [
             [
               plug: {BrowserFixture, [token: session(), owner: self()]},
               port: 4345,
               ip: {127, 0, 0, 1}
             ]
           ]}
      })

      IO.puts("Synthetic connection browser fixture ready on http://127.0.0.1:4345")

      receive do
        :fixture_stop -> :ok
      after
        240_000 -> :ok
      end
    end)
  end

  defp provision do
    assert {:ok, %{status: 201}} =
             Tesla.post(RustCoreClient.write_client(), "/api/v1/tenants", %{
               id: @tenant,
               name: "Synthetic connections"
             })

    set_members([%{"user_id" => @subject, "role" => "member"}])

    set_metadata(%{
      "subscription" => %{"tier" => "indie", "status" => "active"},
      "quotas" => %{"mcp_scope" => "read", "queries_quota" => 100, "queries_used" => 0}
    })
  end

  defp set_members(members) do
    assert {:ok, %{status: 200}} =
             Tesla.post(RustCoreClient.write_client(), "/api/v1/config", %{
               key: "team:#{@tenant}:members",
               value: %{"schema_version" => 2, "members" => members, "invitations" => %{}},
               changed_by: "synthetic-test"
             })
  end

  defp set_metadata(metadata) do
    assert {:ok, %{status: 200}} =
             Tesla.put(RustCoreClient.write_client(), "/api/v1/tenants/#{@tenant}", %{
               metadata: metadata
             })
  end

  defp grant_binding,
    do: %{
      "tenant_id" => @tenant,
      "subject_id" => @subject,
      "client_id" => "claude-code",
      "resource" => @resource
    }

  defp session(extra \\ %{}) do
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

  defp request(context, operation, token, body) do
    client =
      Tesla.client(
        [
          {Tesla.Middleware.BaseUrl, context.query_url},
          {Tesla.Middleware.Headers,
           [{"authorization", "Bearer " <> token}, {"content-type", "application/json"}]},
          {Tesla.Middleware.Timeout, timeout: 10_000}
        ],
        Tesla.Adapter.Hackney
      )

    {:ok, response} =
      Tesla.post(client, "/api/customer-agent/connections/" <> operation, Jason.encode!(body))

    assert {"cache-control", "no-store"} in response.headers
    {response.status, Jason.decode!(response.body)}
  end
end
