defmodule QueryServiceEx.Infrastructure.Adapters.CustomerQueryUsageStoreTest do
  use ExUnit.Case, async: false
  alias QueryServiceEx.Domain.CustomerAgent.QueryAdmission
  alias QueryServiceEx.Infrastructure.Adapters.CustomerQueryUsageStore, as: Store
  @tenant "synthetic-query-meter"
  @path "/api/v1/tenants/" <> @tenant <> "/usage/queries"

  defmodule Fixture do
    import Plug.Conn
    def init(options), do: options

    # Other suites' buffered reporters can flush unrelated tenants.
    # Calls for this test's tenant still get captured, including legacy fallback.
    def call(conn, opts) do
      if unrelated_usage?(conn.request_path, opts[:tenant]),
        do: send_resp(conn, 200, "{}"),
        else: capture(conn, opts)
    end

    defp unrelated_usage?(path, tenant) do
      case Regex.run(~r{/api/v1/tenants/([^/]+)/usage/increment\z}, path, capture: :all_but_first) do
        [other] -> other != tenant
        _ -> false
      end
    end

    defp capture(conn, opts) do
      {:ok, body, conn} = read_body(conn)
      input = if body == "", do: nil, else: Jason.decode!(body)

      send(
        opts[:owner],
        {:request, conn.method, conn.request_path, input, get_req_header(conn, "authorization")}
      )

      {status, response} = opts[:reply].(conn, input)

      conn
      |> put_resp_header("location", "/must-not-follow")
      |> put_resp_content_type("application/json")
      |> send_resp(status, response)
    end
  end

  setup do
    previous =
      for key <- [:core_write_url, :core_read_urls, :core_api_key],
          do: {key, Application.get_env(:query_service_ex, key)}

    on_exit(fn ->
      Enum.each(previous, fn
        {key, nil} -> Application.delete_env(:query_service_ex, key)
        {key, value} -> Application.put_env(:query_service_ex, key, value)
      end)
    end)

    Application.put_env(:query_service_ex, :core_api_key, "Bearer synthetic-admin")
    :ok
  end

  test "snapshot and exact retry receipt come from the leader with service authority" do
    expected = snapshot()

    fixture(fn conn, input ->
      body = if conn.method == "GET", do: expected, else: receipt(input)
      {200, Jason.encode!(body)}
    end)

    assert {:ok, %{"period" => 7, "managed" => true}} = Store.snapshot(@tenant)
    assert_receive {:request, "GET", @path, nil, ["Bearer synthetic-admin"]}
    input = request()
    assert {:ok, %{receipt: returned, replayed: true}} = Store.admit(@tenant, input)
    assert returned == receipt(input)["receipt"]
    admit_path = @path <> "/admit"
    assert_receive {:request, "POST", ^admit_path, ^input, ["Bearer synthetic-admin"]}
    refute_receive {:request, _, _, _, _}
  end

  test "receipt changes to identity, fingerprint, count, period or expiry never authorize work" do
    variants = [
      {"operation_id", "0:00000000-0000-4000-8000-000000000001"},
      {"fingerprint", String.duplicate("b", 64)},
      {"count", 2},
      {"count", 1.0},
      {"period", 8},
      {"expires_at", 0},
      {"used", 0},
      {"used", -1},
      {"used", 18_446_744_073_709_551_616}
    ]

    Enum.each(variants, fn {field, value} ->
      fixture(fn _, input ->
        {200, input |> receipt() |> put_in(["receipt", field], value) |> Jason.encode!()}
      end)

      assert {:error, :query_usage_unavailable} = Store.admit(@tenant, request())
    end)
  end

  test "missing, unknown or legacy protocol fields fail closed" do
    for alter <- [
          &Map.delete(&1, "protocol"),
          &Map.put(&1, "protocol", "legacy"),
          &Map.put(&1, "replayed", "true"),
          &Map.put(&1, "approved", true),
          &Map.put(&1, "status", "pending")
        ] do
      fixture(fn _, input -> {200, input |> receipt() |> alter.() |> Jason.encode!()} end)
      assert {:error, :query_usage_unavailable} = Store.admit(@tenant, request())
    end
  end

  test "a receipt that expires during transport cannot start source work" do
    input = request()
    body = receipt(input)
    expiry = body["receipt"]["expires_at"]
    assert {:ok, _} = QueryAdmission.receipt(body, input, expiry - 1)
    assert {:error, :query_usage_unavailable} = QueryAdmission.receipt(body, input, expiry)
    assert {:error, :query_usage_unavailable} = QueryAdmission.receipt(body, input, expiry + 1)
  end

  test "invalid snapshots cannot establish an unmetered or replacement period" do
    for change <- [
          %{"period" => -1},
          %{"period" => 0.0},
          %{"managed" => "false"},
          %{"quota" => -2},
          %{"used" => -1},
          %{"queries_used" => 0}
        ] do
      fixture(fn _, _ ->
        {200, snapshot() |> update_in(["snapshot"], &Map.merge(&1, change)) |> Jason.encode!()}
      end)

      assert {:error, :query_usage_unavailable} = Store.snapshot(@tenant)
    end
  end

  test "known denial requires its exact HTTP status and fixed body" do
    for {status, code, expected} <- [
          {402, "quota_exceeded", :query_quota_exceeded},
          {403, "inactive_tenant", :access_denied},
          {409, "period_changed", :query_period_changed},
          {409, "operation_conflict", :query_operation_conflict},
          {410, "expired_operation", :query_operation_expired},
          {429, "receipt_capacity", :query_usage_busy},
          {429, "query_usage_busy", :query_usage_busy},
          {200, "quota_exceeded", :query_usage_unavailable},
          {402, "SYNTHETIC PRIVATE", :query_usage_unavailable}
        ] do
      fixture(fn _, _ -> {status, Jason.encode!(%{error: code})} end)
      assert {:error, ^expected} = Store.admit(@tenant, request())
    end
  end

  test "redirect and unavailable leader never forward credentials, retry or use a follower" do
    follower = fixture(fn _, _ -> {200, Jason.encode!(snapshot())} end)
    Application.put_env(:query_service_ex, :core_read_urls, [follower])

    for status <- [307, 503] do
      fixture(fn _, _ -> {status, "SYNTHETIC PRIVATE"} end)
      assert {:error, :query_usage_unavailable} = Store.snapshot(@tenant)
      assert_receive {:request, "GET", @path, nil, _}
      refute_receive {:request, _, _, _, _}
    end
  end

  test "oversized upstream content is refused before decode" do
    fixture(fn _, _ -> {200, String.duplicate("x", 2_049)} end)
    assert {:error, :query_usage_unavailable} = Store.admit(@tenant, request())
  end

  test "invalid caller data cannot become a network request" do
    fixture(fn _, _ -> {200, "{}"} end)
    input = request()
    now = System.system_time(:second)

    for {field, value} <- [
          {"operation_id", "#{now + 3_600}:00000000-0000-4000-8000-000000000001"},
          {"operation_id", "#{now - 3_600}:00000000-0000-4000-8000-000000000001"},
          {"operation_id", "0#{now}:00000000-0000-4000-8000-000000000001"},
          {"fingerprint", String.duplicate("A", 64)},
          {"count", 0},
          {"count", 5},
          {"count", 1.0},
          {"expected_period", -1},
          {"expected_period", 18_446_744_073_709_551_616}
        ] do
      assert {:error, :invalid_query_usage_request} =
               Store.admit(@tenant, Map.put(input, field, value))
    end

    assert {:error, :invalid_query_usage_request} = Store.snapshot("../other")
    assert {:error, :invalid_query_usage_request} = Store.admit(@tenant, nil)

    assert {:error, :invalid_query_usage_request} =
             Store.admit(@tenant, Map.put(input, "quota", -1))

    refute_receive {:request, _, _, _, _}
  end

  test "service URL with embedded credentials or query cannot receive the admin credential" do
    url = fixture(fn _, _ -> {200, "{}"} end)

    for value <- [
          url <> "?token=private",
          String.replace(url, "://", "://private@"),
          url <> "/api"
        ] do
      Application.put_env(:query_service_ex, :core_write_url, value)
      assert {:error, :query_usage_unavailable} = Store.snapshot(@tenant)
    end

    refute_receive {:request, _, _, _, _}
  end

  defp request do
    %{
      "operation_id" => "#{System.system_time(:second)}:00000000-0000-4000-8000-000000000001",
      "fingerprint" => String.duplicate("a", 64),
      "count" => 1,
      "expected_period" => 7
    }
  end

  defp snapshot,
    do: %{
      "protocol" => "canonical-query-usage-v1",
      "snapshot" => %{"period" => 7, "used" => 1, "quota" => 2, "managed" => true}
    }

  defp receipt(input) do
    [issued, _] = String.split(input["operation_id"], ":")

    %{
      "protocol" => "canonical-query-usage-v1",
      "status" => "admitted",
      "replayed" => true,
      "receipt" => %{
        "operation_id" => input["operation_id"],
        "fingerprint" => input["fingerprint"],
        "count" => input["count"],
        "period" => input["expected_period"],
        "used" => 1,
        "expires_at" => String.to_integer(issued) + 3_600
      }
    }
  end

  defp fixture(reply) do
    server =
      start_supervised!(
        {Bandit,
         plug: {Fixture, owner: self(), reply: reply, tenant: @tenant},
         ip: {127, 0, 0, 1},
         port: 0},
        id: make_ref()
      )

    {:ok, {_ip, port}} = ThousandIsland.listener_info(server)
    url = "http://127.0.0.1:#{port}"
    Application.put_env(:query_service_ex, :core_write_url, url)
    url
  end
end
