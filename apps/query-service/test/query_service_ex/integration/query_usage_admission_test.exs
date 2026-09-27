defmodule QueryServiceEx.Integration.QueryUsageAdmissionTest do
  use ExUnit.Case, async: false
  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient
  alias QueryServiceEx.TestSupport.CustomerAgentCore, as: Core
  import QueryServiceEx.TestSupport.CustomerAgentCore, only: [with_core: 2]

  @moduletag :integration
  @moduletag timeout: 60_000
  @moduletag skip: is_nil(System.get_env("ALLSOURCE_CORE_BINARY"))
  @tenant "grant-test-tenant"
  @tenant_path "/api/v1/tenants/" <> @tenant
  @usage_path @tenant_path <> "/usage/queries/"

  setup do
    Core.setup_context()
  end

  test "durable admission recovers its exact receipt after hard restart", context do
    input = request(2)

    original =
      with_core(context, fn ->
        create(10)
        assert {:ok, %{status: 200, body: body}} = admit(input)
        assert body["protocol"] == "canonical-query-usage-v1"
        assert body["replayed"] == false
        assert body["receipt"]["used"] == 2
        body["receipt"]
      end)

    with_core(context, fn ->
      assert {:ok, %{status: 200, body: body}} = admit(input)
      assert body["replayed"] == true
      assert body["receipt"] == original
      assert used() == 2
      assert {:ok, %{status: 409}} = admit(%{input | count: 1})
      assert used() == 2
    end)
  end

  test "the final query unit has one winner across simultaneous HTTP requests", context do
    with_core(context, fn ->
      create(1)

      results =
        1..8
        |> Task.async_stream(fn _ -> admit(request(1)) end, max_concurrency: 8, timeout: 10_000)
        |> Enum.map(fn {:ok, {:ok, response}} -> response.status end)

      assert Enum.count(results, &(&1 == 200)) == 1
      assert Enum.count(results, &(&1 == 402)) == 7
      assert used() == 1
    end)
  end

  test "stale writes and repeated reset cannot erase admitted queries", context do
    next = DateTime.utc_now() |> DateTime.add(30 * 86_400) |> DateTime.to_iso8601()
    transition = %{expected_period: 0}

    with_core(context, fn ->
      create(10)
      assert {:ok, %{status: 200, body: initial}} = Tesla.get(client(), @tenant_path)
      assert {:ok, %{status: 200}} = admit(request(2))

      assert {:ok, %{status: 200, body: saved}} =
               Tesla.put(client(), @tenant_path, %{metadata: initial["metadata"]})

      assert saved["metadata"]["quotas"]["queries_used"] == 2

      assert {:ok, %{status: 200, body: patched}} =
               Tesla.patch(client(), @tenant_path <> "/metadata", %{
                 quotas: %{queries_used: 0, reset_date: next},
                 projections: %{enabled: ["synthetic"]}
               })

      assert patched["metadata"]["quotas"]["queries_used"] == 2
      assert patched["metadata"]["quotas"]["reset_date"] == next

      assert {:ok, %{status: 200, body: reset}} =
               Tesla.post(client(), @usage_path <> "reset", transition)

      assert reset["replayed"] == false
      assert {:ok, %{status: 200}} = admit(%{request(1) | expected_period: 1})
    end)

    with_core(context, fn ->
      assert {:ok, %{status: 200, body: reset}} =
               Tesla.post(client(), @usage_path <> "reset", transition)

      assert reset["replayed"] == true
      assert used() == 1

      assert {:ok, %{status: 200, body: snapshot}} =
               Tesla.get(client(), @tenant_path <> "/usage/queries")

      assert snapshot["snapshot"]["period"] == 1
      assert snapshot["snapshot"]["used"] == 1
      assert {:ok, %{status: 200, body: tenant}} = Tesla.get(client(), @tenant_path)
      assert tenant["metadata"]["quotas"]["reset_date"] == next
      assert tenant["metadata"]["quotas"]["events_used"] == 7
      assert tenant["metadata"]["quotas"]["x402_used"] == 4
      assert {:ok, %{status: 409}} = admit(request(1))
      assert used() == 1
    end)
  end

  test "agent credentials and malformed or oversized requests cannot charge", context do
    with_core(context, fn ->
      create(10)

      restricted =
        Tesla.client([
          {Tesla.Middleware.BaseUrl, context.url},
          Tesla.Middleware.JSON,
          {Tesla.Middleware.Headers, [{"authorization", "Bearer " <> Core.token("developer")}]}
        ])

      # Core's existing route middleware rejects non-admin access with 401.
      assert {:ok, %{status: 401}} = Tesla.post(restricted, @usage_path <> "admit", request(1))
      assert {:ok, %{status: 401}} = Tesla.post(restricted, @usage_path <> "reset", %{})
      assert {:ok, %{status: 401}} = Tesla.get(restricted, @tenant_path <> "/usage/queries")

      assert {:ok, %{status: 401}} =
               Tesla.patch(restricted, @tenant_path <> "/metadata", %{
                 quotas: %{queries_quota: -1, queries_used: 0}
               })

      assert {:ok, %{status: 401}} =
               Tesla.patch(restricted, @tenant_path <> "/metadata", %{
                 projections: %{enabled: ["synthetic"]}
               })

      assert {:ok, %{status: 422}} = admit(Map.put(request(1), :quota, -1))

      assert {:ok, %{status: 413}} =
               admit(%{request(1) | fingerprint: String.duplicate("x", 3_000)})

      assert {:ok, %{status: 400}} = admit(%{request(1) | count: 0})
      old = "#{System.system_time(:second) - 3_601}:00000000-0000-4000-8000-000000000001"
      assert {:ok, %{status: 410}} = admit(%{request(1) | operation_id: old})
      assert used() == 0
    end)
  end

  defp create(limit) do
    assert {:ok, %{status: 201}} =
             Tesla.post(client(), "/api/v1/tenants", %{
               id: @tenant,
               name: "Synthetic query meter",
               metadata: %{
                 quotas: %{queries_quota: limit, queries_used: 0, events_used: 7, x402_used: 4}
               }
             })
  end

  defp request(count) do
    nonce = :crypto.strong_rand_bytes(16) |> Base.encode16(case: :lower)

    <<a::binary-size(8), b::binary-size(4), c::binary-size(4), d::binary-size(4), e::binary>> =
      nonce

    %{
      operation_id: "#{System.system_time(:second)}:#{a}-#{b}-#{c}-#{d}-#{e}",
      fingerprint: String.duplicate("a", 64),
      count: count,
      expected_period: 0
    }
  end

  defp client, do: RustCoreClient.write_client()
  defp admit(input), do: Tesla.post(client(), @usage_path <> "admit", input)

  defp used do
    assert {:ok, %{status: 200, body: body}} = Tesla.get(client(), @tenant_path)
    body["metadata"]["quotas"]["queries_used"]
  end
end
