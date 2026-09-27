defmodule QueryServiceEx.Infrastructure.Adapters.AgentRunStoreTest do
  use ExUnit.Case, async: false
  alias QueryServiceEx.Domain.AgentRun.Event
  alias QueryServiceEx.Infrastructure.Adapters.AgentRunStore
  alias QueryServiceEx.TestSupport.AgentRunFixture, as: F

  defmodule Fixture do
    import Plug.Conn
    def init(opts), do: opts

    def call(conn, opts) do
      conn = fetch_query_params(conn)

      send(
        opts[:owner],
        {:source_read, conn.method, conn.query_params, get_req_header(conn, "authorization")}
      )

      conn |> put_resp_content_type("application/json") |> send_resp(opts[:status], opts[:body])
    end
  end

  setup do
    keys = [:core_write_url, :core_api_key]
    previous = Enum.map(keys, &{&1, Application.get_env(:query_service_ex, &1)})

    on_exit(fn ->
      Enum.each(previous, fn
        {key, nil} -> Application.delete_env(:query_service_ex, key)
        {key, value} -> Application.put_env(:query_service_ex, key, value)
      end)
    end)

    Application.put_env(:query_service_ex, :core_api_key, "Bearer synthetic-only")
    :ok
  end

  test "fixed leader query carries only authoritative tenant and bounded run handle" do
    fixture(200, Jason.encode!(%{events: [], count: 0, total_count: 0, has_more: false}))
    assert {:ok, []} = AgentRunStore.events(F.tenant(), F.uuid(1))
    assert_receive {:source_read, "GET", params, ["Bearer synthetic-only"]}

    assert params == %{
             "tenant_id" => F.tenant(),
             "entity_id" => Event.entity(F.tenant(), F.uuid(1)),
             "limit" => "1001"
           }

    refute_receive {:source_read, _, _, _}
  end

  test "streaming body limit aborts before JSON decode" do
    fixture(200, String.duplicate("x", 2_097_153))
    assert {:error, :run_too_large} = AgentRunStore.events(F.tenant(), F.uuid(1))
    assert_receive {:source_read, _, _, _}
    refute_receive {:source_read, _, _, _}
  end

  test "failed upstream has no retry and fixed error without response body" do
    fixture(503, "synthetic private response")
    assert {:error, :source_unavailable} = AgentRunStore.events(F.tenant(), F.uuid(1))
    assert_receive {:source_read, _, _, _}
    refute_receive {:source_read, _, _, _}
  end

  test "partial history cannot become a complete evidence record" do
    fixture(200, Jason.encode!(%{events: [], count: 0, total_count: 1, has_more: true}))
    assert {:error, :source_unavailable} = AgentRunStore.events(F.tenant(), F.uuid(1))
  end

  test "invalid tenant or run is denied before network access" do
    fixture(200, "{}")

    for {tenant, run} <- [{"a&tenant_id=b", F.uuid(1)}, {F.tenant(), "../private"}, {nil, nil}] do
      assert {:error, :source_unavailable} = AgentRunStore.events(tenant, run)
    end

    refute_receive {:source_read, _, _, _}
  end

  defp fixture(status, body) do
    server =
      start_supervised!(
        {Bandit,
         plug: {Fixture, owner: self(), status: status, body: body}, ip: {127, 0, 0, 1}, port: 0}
      )

    {:ok, {_ip, port}} = ThousandIsland.listener_info(server)
    Application.put_env(:query_service_ex, :core_write_url, "http://127.0.0.1:#{port}")
  end
end
