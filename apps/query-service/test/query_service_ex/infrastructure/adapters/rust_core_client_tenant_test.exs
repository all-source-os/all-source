defmodule QueryServiceEx.Infrastructure.Adapters.RustCoreClientTenantTest do
  @moduledoc """
  The gateway's tenant stamp, asserted on the WIRE.

  `query_events/3` and `query_events_page/3` are the single place the
  authenticated tenant is attached to a Core read, so they are the place tenant
  isolation is either enforced or lost. Asserting on the decoded params would
  miss the actual defect — the map can legitimately hold `"tenant_id"` and
  `:tenant_id` at once, and only the encoded query string shows that BOTH went
  out. So these tests read `conn.query_string` verbatim.

  `QueryController.build_simple_query/1` returns the request map untouched, so a
  client-supplied `tenant_id` reaches this seam as a string key on a real
  request path — this is not a hypothetical input.
  """
  use ExUnit.Case, async: false

  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient

  defmodule FakeCore do
    @moduledoc false
    import Plug.Conn

    def init(opts), do: opts

    def call(%{method: "GET", request_path: "/api/v1/events/query"} = conn, _opts) do
      :ok = Agent.update(FakeCoreState, &[conn.query_string | &1])

      conn
      |> put_resp_content_type("application/json")
      |> send_resp(200, Jason.encode!(%{"events" => [], "count" => 0}))
    end

    def call(%{method: "GET", request_path: "/health"} = conn, _opts) do
      conn
      |> put_resp_content_type("application/json")
      |> send_resp(200, Jason.encode!(%{"status" => "healthy"}))
    end

    def call(conn, _opts), do: send_resp(conn, 404, "not found")
  end

  setup do
    start_supervised!(%{
      id: FakeCoreState,
      start: {Agent, :start_link, [fn -> [] end, [name: FakeCoreState]]}
    })

    port = free_port()
    start_supervised!({Bandit, plug: FakeCore, scheme: :http, port: port, ip: {127, 0, 0, 1}})

    url = "http://127.0.0.1:#{port}"
    prev = Enum.map([:core_url, :core_read_urls, :core_write_url], &{&1, get_env(&1)})

    Application.put_env(:query_service_ex, :core_url, url)
    Application.put_env(:query_service_ex, :core_read_urls, [url])
    Application.put_env(:query_service_ex, :core_write_url, url)

    on_exit(fn -> Enum.each(prev, fn {key, value} -> restore_env(key, value) end) end)

    :ok
  end

  describe "query_events/3" do
    test "a client-supplied tenant_id never reaches Core" do
      assert {:ok, []} =
               RustCoreClient.query_events("authenticated-tenant", %{
                 "tenant_id" => "victim-tenant",
                 "limit" => 5
               })

      assert_single_tenant("authenticated-tenant")
    end

    test "keeps the caller's other params while replacing the tenant" do
      assert {:ok, []} =
               RustCoreClient.query_events("authenticated-tenant", %{
                 "tenant_id" => "victim-tenant",
                 "entity_id" => "order-1"
               })

      params = decoded_params()
      assert params["entity_id"] == "order-1"
      assert params["tenant_id"] == "authenticated-tenant"
    end

    test "an atom tenant_id in the caller's map is still overridden" do
      assert {:ok, []} =
               RustCoreClient.query_events("authenticated-tenant", %{
                 tenant_id: "victim-tenant",
                 limit: 1
               })

      assert_single_tenant("authenticated-tenant")
    end
  end

  describe "query_events_page/3" do
    test "a client-supplied tenant_id never reaches Core" do
      assert {:ok, %{"events" => []}} =
               RustCoreClient.query_events_page("authenticated-tenant", %{
                 "tenant_id" => "victim-tenant",
                 "limit" => 5
               })

      assert_single_tenant("authenticated-tenant")
    end

    test "an unrelated health request cannot overwrite the tenant query observation" do
      assert {:ok, _} =
               RustCoreClient.query_events_page("authenticated-tenant", %{
                 "tenant_id" => "victim-tenant",
                 "limit" => 5
               })

      assert {:ok, %{status: 200}} = Tesla.get(RustCoreClient.write_client(), "/health")
      assert_single_tenant("authenticated-tenant")
    end
  end

  defp assert_single_tenant(expected) do
    query_string = observed_query()

    tenants =
      query_string
      |> URI.query_decoder()
      |> Enum.filter(fn {key, _} -> key == "tenant_id" end)
      |> Enum.map(&elem(&1, 1))

    assert tenants == [expected],
           "expected exactly one tenant_id on the wire, got #{inspect(tenants)} " <>
             "from #{inspect(query_string)} — two tenant_id pairs means the gateway " <>
             "appended its tenant instead of overriding the caller's"
  end

  defp decoded_params do
    observed_query() |> URI.decode_query()
  end

  defp observed_query do
    queries = Agent.get(FakeCoreState, & &1)
    assert [query] = queries, "expected exactly one event query, got #{inspect(queries)}"
    query
  end

  defp get_env(key), do: Application.get_env(:query_service_ex, key)

  defp restore_env(key, nil), do: Application.delete_env(:query_service_ex, key)
  defp restore_env(key, value), do: Application.put_env(:query_service_ex, key, value)

  defp free_port do
    {:ok, socket} = :gen_tcp.listen(0, [:binary, active: false, ip: {127, 0, 0, 1}])
    {:ok, port} = :inet.port(socket)
    :ok = :gen_tcp.close(socket)
    port
  end
end
