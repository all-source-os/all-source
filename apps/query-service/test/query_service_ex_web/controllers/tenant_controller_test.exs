defmodule QueryServiceExWeb.TenantControllerTest do
  @moduledoc """
  Tests for TenantController.

  No external database required - uses plain maps and TenantCache.
  """
  use QueryServiceExWeb.ConnCase

  alias QueryServiceEx.AuthHelpers
  alias QueryServiceEx.TenantCache

  setup %{conn: conn} do
    System.put_env("JWT_SECRET", AuthHelpers.test_jwt_secret())
    on_exit(fn -> System.delete_env("JWT_SECRET") end)

    tenant_id = "tenant-test-#{:rand.uniform(100_000)}"

    tenant = %{
      "id" => tenant_id,
      "name" => "Test Workspace",
      "slug" => "test-workspace",
      "status" => "active",
      "metadata" => %{
        "subscription" => %{
          "status" => "active",
          "tier" => "free",
          "trial_ends_at" => nil,
          "subscription_ends_at" => nil
        },
        "quotas" => %{
          "events_quota" => 10_000,
          "queries_quota" => 1_000,
          "events_used" => 0,
          "queries_used" => 0
        }
      }
    }

    TenantCache.put(tenant_id, tenant)
    on_exit(fn -> TenantCache.clear() end)

    {user, token} = AuthHelpers.create_test_user_with_token(%{tenant_id: tenant_id})

    conn =
      conn
      |> put_req_header("authorization", "Bearer #{token}")
      |> put_req_header("accept", "application/json")

    {:ok, conn: conn, tenant: tenant, user: user, tenant_id: tenant_id}
  end

  describe "GET /api/tenant" do
    test "returns current tenant info", %{conn: conn, tenant: tenant} do
      conn = get(conn, "/api/tenant")
      response = json_response(conn, 200)["data"]

      assert response["id"] == tenant["id"]
      assert response["name"] == tenant["name"]
    end

    test "returns 401 without auth token", %{conn: conn} do
      conn =
        conn
        |> delete_req_header("authorization")
        |> get("/api/tenant")

      assert json_response(conn, 401)
    end
  end

  describe "PUT /api/tenant" do
    test "returns current tenant data (writes go to Control Plane)", %{conn: conn, tenant: tenant} do
      conn = put(conn, "/api/tenant", %{"name" => "Updated Workspace"})
      response = json_response(conn, 200)["data"]

      # Update is read-only in QS — returns cached tenant data
      assert response["id"] == tenant["id"]
      assert response["name"] == tenant["name"]
    end
  end

  describe "GET /api/tenant/usage" do
    test "returns usage statistics", %{conn: conn} do
      conn = get(conn, "/api/tenant/usage")
      response = json_response(conn, 200)["data"]

      assert response["events"]["used"] == 0
      assert response["events"]["quota"] == 10_000
      assert response["queries"]["used"] == 0
      assert response["queries"]["quota"] == 1_000
    end

    test "reports extraction tokens against the allowance", %{conn: conn, tenant: tenant} do
      quotas =
        Map.merge(tenant["metadata"]["quotas"], %{
          "extraction_tokens_quota" => 500_000,
          "extraction_tokens_used" => 12_000
        })

      TenantCache.put(tenant["id"], put_in(tenant, ["metadata", "quotas"], quotas))

      response = conn |> get("/api/tenant/usage") |> json_response(200) |> Map.fetch!("data")

      assert response["extraction_tokens"] == %{
               "used" => 12_000,
               "quota" => 500_000,
               "remaining" => 488_000
             }
    end

    test "a tenant with no extraction allowance reports zero, not unlimited", %{conn: conn} do
      response = conn |> get("/api/tenant/usage") |> json_response(200) |> Map.fetch!("data")

      assert response["extraction_tokens"] == %{"used" => 0, "quota" => 0, "remaining" => 0}
    end
  end
end
