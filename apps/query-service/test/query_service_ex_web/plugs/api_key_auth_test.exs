defmodule QueryServiceExWeb.Plugs.ApiKeyAuthTest do
  use ExUnit.Case, async: false

  import Plug.Conn
  import Phoenix.ConnTest

  alias QueryServiceEx.ApiKeyCache
  alias QueryServiceExWeb.Plugs.ApiKeyAuth

  setup do
    ApiKeyCache.clear()
    :ok
  end

  describe "ApiKeyAuth plug" do
    test "returns 401 when X-API-Key header is missing" do
      conn =
        build_conn()
        |> ApiKeyAuth.call(ApiKeyAuth.init([]))

      assert conn.halted
      assert conn.status == 401

      body = json_response(conn, 401)
      assert body["error"]["code"] == "unauthorized"
      assert body["error"]["message"] =~ "missing"
    end

    test "returns 401 when X-API-Key header is empty" do
      conn =
        build_conn()
        |> put_req_header("x-api-key", "")
        |> ApiKeyAuth.call(ApiKeyAuth.init([]))

      assert conn.halted
      assert conn.status == 401

      body = json_response(conn, 401)
      assert body["error"]["code"] == "unauthorized"
      assert body["error"]["message"] =~ "empty"
    end

    test "bypasses auth in dev mode" do
      original = System.get_env("AUTH_DISABLED")
      System.put_env("AUTH_DISABLED", "true")

      conn =
        build_conn()
        |> ApiKeyAuth.call(ApiKeyAuth.init([]))

      refute conn.halted

      if original,
        do: System.put_env("AUTH_DISABLED", original),
        else: System.delete_env("AUTH_DISABLED")
    end

    test "cached identity cannot authenticate an invalid credential" do
      tenant = %{
        "id" => "tenant-abc",
        "name" => "Test Tenant",
        "status" => "active",
        "metadata" => %{"subscription" => %{"status" => "active"}}
      }

      api_key = %{
        "id" => "key-123",
        "tenant_id" => "tenant-abc",
        "name" => "test-key",
        "scopes" => ["events:read", "events:write"]
      }

      # Pre-populate the cache with a verified result
      raw_key = "allsource_test_fakekeyfortest1234567890ab"
      cache_key = :crypto.hash(:sha256, raw_key) |> Base.encode16(case: :lower)

      :ets.insert(
        :api_key_cache,
        {cache_key, {tenant, api_key}, System.system_time(:second) + 120}
      )

      conn =
        build_conn()
        |> put_req_header("x-api-key", raw_key)
        |> ApiKeyAuth.call(ApiKeyAuth.init([]))

      assert conn.halted
      assert conn.status == 401
      refute conn.assigns[:api_key]
    end

    test "cached subscription cannot bypass credential verification" do
      tenant = %{
        "id" => "tenant-suspended",
        "name" => "Suspended Tenant",
        "status" => "suspended",
        "metadata" => %{"subscription" => %{"status" => "cancelled"}}
      }

      api_key = %{
        "id" => "key-456",
        "tenant_id" => "tenant-suspended",
        "name" => "suspended-key",
        "scopes" => ["events:read"]
      }

      raw_key = "allsource_test_suspendedkey12345678901234"
      cache_key = :crypto.hash(:sha256, raw_key) |> Base.encode16(case: :lower)

      :ets.insert(
        :api_key_cache,
        {cache_key, {tenant, api_key}, System.system_time(:second) + 120}
      )

      conn =
        build_conn()
        |> put_req_header("x-api-key", raw_key)
        |> ApiKeyAuth.call(ApiKeyAuth.init([]))

      assert conn.halted
      assert conn.status == 401

      body = json_response(conn, 401)
      assert body["error"]["code"] == "unauthorized"
    end

    test "returns 401 for invalid key via ApiKeyCache error passthrough" do
      raw_key = "allsource_test_nonexistentkey123456789"

      result = ApiKeyCache.fetch(raw_key, fn -> {:error, :invalid_key} end)
      assert result == {:error, :invalid_key}

      result2 = ApiKeyCache.fetch(raw_key, fn -> {:error, :key_revoked} end)
      assert result2 == {:error, :key_revoked}

      result3 = ApiKeyCache.fetch(raw_key, fn -> {:error, :key_expired} end)
      assert result3 == {:error, :key_expired}
    end
  end

  describe "ApiKeyCache" do
    test "caches successful verifications" do
      tenant = %{"id" => "t1", "name" => "T", "status" => "active"}
      api_key = %{"id" => "k1", "tenant_id" => "t1", "name" => "k", "scopes" => []}

      call_count = :counters.new(1, [:atomics])

      raw_key = "allsource_test_cachetest123456789012345"

      # First call should invoke the function
      result1 =
        ApiKeyCache.fetch(raw_key, fn ->
          :counters.add(call_count, 1, 1)
          {:ok, {tenant, api_key}}
        end)

      assert {:ok, {^tenant, ^api_key}} = result1
      assert :counters.get(call_count, 1) == 1

      # Second call should use cache
      result2 =
        ApiKeyCache.fetch(raw_key, fn ->
          :counters.add(call_count, 1, 1)
          {:ok, {tenant, api_key}}
        end)

      assert {:ok, {^tenant, ^api_key}} = result2
      assert :counters.get(call_count, 1) == 1
    end

    test "does not cache errors" do
      call_count = :counters.new(1, [:atomics])
      raw_key = "allsource_test_errorkey1234567890123456"

      result1 =
        ApiKeyCache.fetch(raw_key, fn ->
          :counters.add(call_count, 1, 1)
          {:error, :invalid_key}
        end)

      assert result1 == {:error, :invalid_key}
      assert :counters.get(call_count, 1) == 1

      # Should call again since errors aren't cached
      result2 =
        ApiKeyCache.fetch(raw_key, fn ->
          :counters.add(call_count, 1, 1)
          {:error, :invalid_key}
        end)

      assert result2 == {:error, :invalid_key}
      assert :counters.get(call_count, 1) == 2
    end

    test "clear removes all entries" do
      tenant = %{"id" => "t1", "name" => "T", "status" => "active"}
      api_key = %{"id" => "k1", "tenant_id" => "t1", "name" => "k", "scopes" => []}

      raw_key = "allsource_test_cleartest1234567890123456"

      ApiKeyCache.fetch(raw_key, fn -> {:ok, {tenant, api_key}} end)

      ApiKeyCache.clear()

      call_count = :counters.new(1, [:atomics])

      ApiKeyCache.fetch(raw_key, fn ->
        :counters.add(call_count, 1, 1)
        {:ok, {tenant, api_key}}
      end)

      # Should have called the function again after clear
      assert :counters.get(call_count, 1) == 1
    end
  end
end
