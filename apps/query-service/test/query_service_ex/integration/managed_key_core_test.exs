defmodule QueryServiceEx.Integration.ManagedKeyCoreTest do
  use ExUnit.Case, async: false

  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient
  alias QueryServiceEx.TestSupport.CustomerAgentCore

  @moduletag :integration
  @moduletag timeout: 90_000
  @moduletag skip: is_nil(System.get_env("ALLSOURCE_CORE_BINARY"))
  @tenant "grant-test-tenant"

  setup do
    context = CustomerAgentCore.setup_context()
    previous = System.get_env("JWT_SECRET")
    disabled = System.get_env("AUTH_DISABLED")
    System.put_env("JWT_SECRET", "synthetic-only-core-grant-recovery-secret-2026")
    System.delete_env("AUTH_DISABLED")

    server =
      start_supervised!({Bandit, plug: QueryServiceExWeb.Endpoint, port: 0, ip: {127, 0, 0, 1}})

    {:ok, {_, port}} = ThousandIsland.listener_info(server)

    on_exit(fn ->
      for {key, value} <- [{"JWT_SECRET", previous}, {"AUTH_DISABLED", disabled}] do
        if value, do: System.put_env(key, value), else: System.delete_env(key)
      end

      :ets.delete(:api_key_store, {:keys, @tenant})
    end)

    Map.put(context, :query_url, "http://127.0.0.1:#{port}")
  end

  test "HTTP mint, application event, revoke and restart preserve lifecycle authority", context do
    {token, reader} =
      CustomerAgentCore.with_core(context, fn ->
        assert {:ok, %{status: 201}} =
                 Tesla.post(RustCoreClient.write_client(), "/api/v1/tenants", %{
                   id: @tenant,
                   name: "Synthetic lifecycle proof",
                   quota_preset: "unlimited",
                   metadata: %{
                     quotas: %{
                       events_quota: 100,
                       queries_quota: 100,
                       events_used: 0,
                       queries_used: 0
                     }
                   }
                 })

        assert {:ok, %{status: 200, body: %{"protocol" => "funded-egress-v1"}}} =
                 Tesla.post(
                   RustCoreClient.write_client(),
                   "/api/v1/tenants/" <> @tenant <> "/egress",
                   %{
                     operation: "configure",
                     included_bytes: 1_048_576
                   }
                 )

        human = CustomerAgentCore.token("developer")
        expiry = DateTime.add(DateTime.utc_now(), 600) |> DateTime.truncate(:second)

        assert {201, created} =
                 http(context, :post, "/api/api-keys", human, %{
                   name: "synthetic",
                   scopes: ["events:read", "events:write"],
                   expires_at: DateTime.to_iso8601(expiry)
                 })

        assert {:ok, claims} = RustCoreClient.decode_api_key_jwt(created["key"])
        assert claims["exp"] == DateTime.to_unix(expiry)
        token = created["key"]

        assert {201, _} =
                 http(context, :post, "/api/v1/events", token, %{
                   entity_id: "application-proof",
                   event_type: "synthetic.received",
                   payload: %{proof: true}
                 })

        assert {200, %{"events" => [%{"event_type" => "synthetic.received"}]}} =
                 http(context, :get, "/api/v1/events/query?entity_id=application-proof", token)

        assert {403, _} =
                 http(context, :post, "/api/v1/events", human, %{
                   entity_id: "apikey:" <> @tenant,
                   event_type: "apikey.created",
                   payload: %{}
                 })

        assert {403, _} = http(context, :get, "/api/api-keys", token)

        assert {201, reader} =
                 http(context, :post, "/api/api-keys", human, %{
                   name: "synthetic-reader",
                   scopes: ["events:read"]
                 })

        assert is_nil(reader["expires_at"])
        assert {:ok, reader_claims} = RustCoreClient.decode_api_key_jwt(reader["key"])
        refute Map.has_key?(reader_claims, "exp")

        assert {403, _} =
                 http(context, :post, "/api/v1/events", reader["key"], %{
                   entity_id: "application-proof",
                   event_type: "synthetic.denied",
                   payload: %{}
                 })

        assert {200, _} = http(context, :delete, "/api/api-keys/" <> created["id"], human)

        assert {401, _} =
                 http(context, :get, "/api/v1/events/query?entity_id=application-proof", token)

        {token, reader["key"]}
      end)

    CustomerAgentCore.with_core(context, fn ->
      assert {401, _} =
               http(context, :get, "/api/v1/events/query?entity_id=application-proof", token)

      assert {200, %{"events" => [_]}} =
               http(context, :get, "/api/v1/events/query?entity_id=application-proof", reader)
    end)
  end

  defp http(context, method, path, token, body \\ nil) do
    client =
      Tesla.client(
        [
          {Tesla.Middleware.BaseUrl, context.query_url},
          Tesla.Middleware.JSON,
          {Tesla.Middleware.Timeout, timeout: 5_000}
        ],
        Tesla.Adapter.Hackney
      )

    assert {:ok, response} =
             Tesla.request(client,
               method: method,
               url: path,
               headers: [{"authorization", "Bearer " <> token}],
               body: body
             )

    {response.status, response.body}
  end
end
