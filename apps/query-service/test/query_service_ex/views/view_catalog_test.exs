defmodule QueryServiceEx.Views.ViewCatalogTest do
  use ExUnit.Case, async: false

  alias QueryServiceEx.Views.FakeCore
  alias QueryServiceEx.Views.ViewCatalog
  alias QueryServiceEx.Views.ViewFolder

  @definition %{
    "source" => %{"prefixes" => ["ops."]},
    "key" => "{entity_id}",
    "ops" => [%{"on" => "ops.", "increment" => %{"n" => 1}}]
  }

  setup do
    start_supervised!(FakeCore)
    n = System.unique_integer([:positive])
    supervisor = :"view_catalog_test_sup_#{n}"
    start_supervised!({DynamicSupervisor, strategy: :one_for_one, name: supervisor})

    opts = [
      enabled: true,
      name: :"view_catalog_test_#{n}",
      core: FakeCore,
      supervisor: supervisor,
      interval_ms: 60_000,
      folder_opts: [core: FakeCore, auto: false, subscribe: false]
    ]

    catalog = start_supervised!({ViewCatalog, opts})
    {:ok, catalog: catalog, tenant: "tenant-#{n}"}
  end

  defp define(views) do
    FakeCore.update(&%{&1 | views: views})
  end

  defp view(tenant, name, generation) do
    %{
      "tenant_id" => tenant,
      "view" => name,
      "generation" => generation,
      "definition" => @definition
    }
  end

  defp folder(tenant, name) do
    case Registry.lookup(QueryServiceEx.ProjectionRegistry, {:view_folder, tenant, name}) do
      [{pid, _value}] -> pid
      [] -> nil
    end
  end

  test "starts one folder per defined view", %{catalog: catalog, tenant: tenant} do
    define([view(tenant, "a", 1), view(tenant, "b", 3)])

    assert ViewCatalog.refresh(catalog) == %{{tenant, "a"} => 1, {tenant, "b"} => 3}
    assert %{generation: 1} = ViewFolder.status(folder(tenant, "a"))
    assert %{generation: 3} = ViewFolder.status(folder(tenant, "b"))
  end

  test "restarts a folder on a new generation", %{catalog: catalog, tenant: tenant} do
    define([view(tenant, "a", 1)])
    ViewCatalog.refresh(catalog)
    old = folder(tenant, "a")

    define([view(tenant, "a", 2)])

    assert ViewCatalog.refresh(catalog) == %{{tenant, "a"} => 2}
    refute Process.alive?(old)
    assert %{generation: 2} = ViewFolder.status(folder(tenant, "a"))
  end

  test "stops the folder of a deleted view", %{catalog: catalog, tenant: tenant} do
    define([view(tenant, "a", 1)])
    ViewCatalog.refresh(catalog)
    old = folder(tenant, "a")

    define([])

    assert ViewCatalog.refresh(catalog) == %{}
    refute Process.alive?(old)
  end

  test "keeps running folders when the listing fails", %{catalog: catalog, tenant: tenant} do
    define([view(tenant, "a", 1)])
    ViewCatalog.refresh(catalog)

    FakeCore.update(&%{&1 | views: :unavailable})

    assert ViewCatalog.refresh(catalog) == %{{tenant, "a"} => 1}
  end

  test "does not start while folding is disabled" do
    assert :ignore = ViewCatalog.start_link(enabled: false)
  end
end
