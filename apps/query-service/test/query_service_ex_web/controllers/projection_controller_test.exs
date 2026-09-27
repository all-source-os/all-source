defmodule QueryServiceExWeb.ProjectionControllerTest do
  @moduledoc """
  Tests for the tenant-scoped, QS-owned ProjectionController.

  Verifies the per-tenant enablement API: list returns only the tenant's enabled
  projections, enable adds to the enabled set and returns `building`, the catalog
  is served, unknown templates are rejected, fail-closed on missing tenant, and
  cross-tenant isolation (tenant A's enable/state never visible to tenant B).

  The Core client is stubbed via `:projections_core_client` (in-memory enabled
  set) and the backfill source via `:tenant_projection_query_fun`, so no HTTP or
  running Core is required.
  """
  use ExUnit.Case, async: false

  import Plug.Test
  import Plug.Conn

  alias QueryServiceEx.Projections.Enablement
  alias QueryServiceEx.Projections.TenantProjections
  alias QueryServiceExWeb.ProjectionController

  # In-memory Core stub: an Agent holding metadata per tenant. Implements the
  # two functions Enablement calls: get_tenant/1 and merge_tenant_metadata/2.
  defmodule StubCore do
    def start, do: Agent.start_link(fn -> %{} end, name: __MODULE__)

    def get_tenant(tenant_id) do
      meta = Agent.get(__MODULE__, &Map.get(&1, tenant_id, %{}))
      {:ok, %{"id" => tenant_id, "metadata" => meta}}
    end

    def merge_tenant_metadata(tenant_id, partial) do
      Agent.update(__MODULE__, fn state ->
        existing = Map.get(state, tenant_id, %{})
        Map.put(state, tenant_id, deep_merge(existing, partial))
      end)

      {:ok, %{}}
    end

    defp deep_merge(a, b) do
      Map.merge(a, b, fn
        _k, va, vb when is_map(va) and is_map(vb) -> deep_merge(va, vb)
        _k, _va, vb -> vb
      end)
    end
  end

  setup do
    TenantProjections.init_tables()

    if Process.whereis(QueryServiceEx.Projections.BackfillSupervisor) == nil do
      {:ok, _} = Task.Supervisor.start_link(name: QueryServiceEx.Projections.BackfillSupervisor)
    end

    if Process.whereis(TenantProjections) == nil do
      {:ok, _} = TenantProjections.start_link([])
    end

    start_supervised!(%{id: StubCore, start: {StubCore, :start, []}})

    Application.put_env(:query_service_ex, :projections_core_client, StubCore)

    Application.put_env(:query_service_ex, :tenant_projection_query_fun, fn _tenant, _params ->
      {:ok, []}
    end)

    on_exit(fn ->
      Application.delete_env(:query_service_ex, :projections_core_client)
      Application.delete_env(:query_service_ex, :tenant_projection_query_fun)
      :ets.delete_all_objects(:tenant_projection_state)
      :ets.delete_all_objects(:tenant_projection_status)
      :ets.delete_all_objects(:tenant_projection_generation)
    end)

    :ok
  end

  defp tenant_conn(method, path, tenant_id, body \\ nil) do
    conn(method, path, body)
    |> put_req_header("content-type", "application/json")
    |> put_private(:phoenix_endpoint, QueryServiceExWeb.Endpoint)
    |> assign(:tenant_id, tenant_id)
  end

  defp body(conn), do: Jason.decode!(conn.resp_body)

  describe "templates/2" do
    test "returns the curated catalog" do
      conn =
        ProjectionController.templates(tenant_conn(:get, "/api/projection-templates", "t1"), %{})

      assert conn.status == 200
      templates = body(conn)["templates"]
      names = templates |> Enum.map(& &1["name"]) |> Enum.sort()

      assert names == [
               "active-entities",
               "entity-activity",
               "event-count",
               "event-type-leaderboard",
               "events-per-day"
             ]

      # every template carries a render-hint kind the UI switches on
      kinds = templates |> Enum.map(& &1["kind"]) |> Enum.uniq() |> Enum.sort()
      assert Enum.all?(kinds, &(&1 in ["counter", "breakdown", "timeseries", "entity_table"]))
    end
  end

  describe "index/2" do
    test "returns empty for a tenant with nothing enabled" do
      conn = ProjectionController.index(tenant_conn(:get, "/api/projections", "t1"), %{})
      assert conn.status == 200
      assert body(conn) == %{"projections" => [], "total" => 0}
    end

    test "returns only the tenant's enabled projections" do
      enable!("t1", "event-count")

      conn = ProjectionController.index(tenant_conn(:get, "/api/projections", "t1"), %{})
      payload = body(conn)
      assert payload["total"] == 1

      assert [%{"name" => "event-count", "status" => status, "kind" => "counter"}] =
               payload["projections"]

      assert status in ["building", "ready"]
    end
  end

  describe "create/2" do
    test "enables a template and returns building" do
      conn =
        ProjectionController.create(
          tenant_conn(:post, "/api/projections", "t1", %{"template" => "event-count"}),
          %{"template" => "event-count"}
        )

      assert conn.status == 201
      proj = body(conn)["projection"]
      assert proj["name"] == "event-count"
      assert proj["status"] in ["building", "ready"]

      # added to the enabled set in (stubbed) Core metadata
      {:ok, %{"metadata" => meta}} = StubCore.get_tenant("t1")
      assert meta["projections"]["enabled"] == ["event-count"]
    end

    test "rejects an unknown template" do
      conn =
        ProjectionController.create(
          tenant_conn(:post, "/api/projections", "t1", %{"template" => "nope"}),
          %{"template" => "nope"}
        )

      assert conn.status == 404
      assert body(conn)["error"] =~ "Unknown projection template"
    end

    test "requires a template field" do
      conn =
        ProjectionController.create(
          tenant_conn(:post, "/api/projections", "t1", %{}),
          %{}
        )

      assert conn.status == 422
    end
  end

  describe "delete/2" do
    test "removes the projection from the enabled set" do
      enable!("t1", "event-count")

      conn =
        ProjectionController.delete(
          tenant_conn(:delete, "/api/projections/event-count", "t1"),
          %{"name" => "event-count"}
        )

      assert conn.status == 200
      {:ok, %{"metadata" => meta}} = StubCore.get_tenant("t1")
      assert meta["projections"]["enabled"] == []
    end
  end

  describe "get_state/2" do
    test "404 when the projection is not enabled" do
      conn =
        ProjectionController.get_state(
          tenant_conn(:get, "/api/projections/event-count/state", "t1"),
          %{"name" => "event-count", "entity_id" => "_tenant"}
        )

      assert conn.status == 404
    end
  end

  describe "metadata merge contract" do
    test "enabling sends only the projections partial and preserves sibling quotas" do
      # Seed pre-existing quotas that must survive the deep-merge.
      StubCore.merge_tenant_metadata("t1", %{"quotas" => %{"events_used" => 42}})

      enable!("t1", "event-count")

      {:ok, %{"metadata" => meta}} = StubCore.get_tenant("t1")
      assert meta["projections"]["enabled"] == ["event-count"]
      # quotas preserved — the merge body was a partial {"projections" => ...}
      assert meta["quotas"]["events_used"] == 42
    end
  end

  describe "cross-tenant isolation" do
    test "tenant A's enable is not visible to tenant B" do
      enable!("tenant-A", "event-count")

      conn_b = ProjectionController.index(tenant_conn(:get, "/api/projections", "tenant-B"), %{})
      assert body(conn_b) == %{"projections" => [], "total" => 0}

      conn_a = ProjectionController.index(tenant_conn(:get, "/api/projections", "tenant-A"), %{})
      assert body(conn_a)["total"] == 1
    end
  end

  describe "fail-closed" do
    test "index raises when no tenant on the conn" do
      conn =
        conn(:get, "/api/projections")
        |> put_private(:phoenix_endpoint, QueryServiceExWeb.Endpoint)

      assert_raise RuntimeError, ~r/Tenant context required/, fn ->
        ProjectionController.index(conn, %{})
      end
    end

    test "create raises when no tenant on the conn" do
      conn =
        conn(:post, "/api/projections")
        |> put_private(:phoenix_endpoint, QueryServiceExWeb.Endpoint)

      assert_raise RuntimeError, ~r/Tenant context required/, fn ->
        ProjectionController.create(conn, %{"template" => "event-count"})
      end
    end
  end

  defp enable!(tenant, template) do
    {:ok, _} = Enablement.enable(tenant, template)
    :ok
  end
end
