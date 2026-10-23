defmodule QueryServiceExWeb.EventControllerQueryTest do
  @moduledoc """
  `GET /api/v1/events/query` through the REAL router — auth pipeline, tenant
  resolution, controller and Core hop included.

  Previously this file asserted `conn.status in [200, 400, 401]` and wrapped every
  body check in `if conn.status == 200`, against a Core that was never running in
  the test env. It passed for every possible behaviour of the endpoint, including
  the #252 regression it looks like it guards. It now pins the two things that can
  actually break: the route is fail-closed without credentials, and an
  authenticated request gets Core's wire envelope back — pagination metadata
  included — rather than the HATEOAS `data` shape of the `/events/:id` show action.
  """
  use ExUnit.Case, async: false

  import Plug.Test, only: [conn: 3]

  alias QueryServiceExWeb.Router

  @opts Router.init([])

  @tenant "tenant-events-query-test"

  @event %{
    "id" => "11111111-1111-1111-1111-111111111111",
    "entity_id" => "order-1",
    "event_type" => "order.placed",
    "payload" => %{"amount" => 10},
    "timestamp" => "2026-01-01T00:00:00Z",
    "version" => 1
  }

  # Core's envelope for a first page that has more behind it. `total_count` and
  # `has_more` are the fields #252 dropped on the floor at the gateway.
  @core_body %{
    "events" => [@event],
    "count" => 1,
    "total_count" => 9,
    "has_more" => true
  }

  defmodule FakeCore do
    @moduledoc false
    import Plug.Conn

    def init(opts), do: opts

    def call(%{method: "POST", request_path: "/api/v1/tenants/" <> _} = conn, opts) do
      QueryServiceEx.ChannelEgressFixture.call(conn, opts)
    end

    def call(%{request_path: "/api/v1/events/query"} = conn, _opts) do
      conn = fetch_query_params(conn)
      body = Agent.get(FakeCoreState, & &1.body)
      :ok = Agent.update(FakeCoreState, &%{&1 | query: conn.query_params})

      json(conn, 200, body)
    end

    # TenantContext resolves the JWT's tenant through Core before the controller
    # runs; without this the request 404s in the plug and never reaches the seam.
    def call(%{request_path: "/api/v1/tenants/" <> tenant_id} = conn, _opts) do
      json(conn, 200, %{
        "tenant_id" => tenant_id,
        "id" => tenant_id,
        "name" => "Test",
        "metadata" => %{"subscription" => %{"status" => "active"}}
      })
    end

    def call(conn, _opts), do: json(conn, 404, %{"error" => "not found"})

    defp json(conn, status, body) do
      conn
      |> put_resp_content_type("application/json")
      |> send_resp(status, Jason.encode!(body))
    end
  end

  setup do
    start_supervised!(%{
      id: FakeCoreState,
      start:
        {Agent, :start_link, [fn -> %{query: %{}, body: @core_body} end, [name: FakeCoreState]]}
    })

    port = free_port()
    ledger = start_supervised!({Agent, fn -> %{left: 1_000_000, holds: %{}} end})

    start_supervised!(
      {Bandit, plug: {FakeCore, ledger: ledger}, scheme: :http, port: port, ip: {127, 0, 0, 1}}
    )

    url = "http://127.0.0.1:#{port}"
    prev = Enum.map([:core_url, :core_read_urls, :core_write_url], &{&1, get_env(&1)})
    prev_secret = System.get_env("JWT_SECRET")
    prev_auth_disabled = System.get_env("AUTH_DISABLED")

    Application.put_env(:query_service_ex, :core_url, url)
    Application.put_env(:query_service_ex, :core_read_urls, [url])
    Application.put_env(:query_service_ex, :core_write_url, url)

    # A dev-mode bypass would assign a tenant for us and hide the fail-closed check.
    System.delete_env("AUTH_DISABLED")
    System.put_env("JWT_SECRET", jwt_secret())
    QueryServiceEx.TenantCache.invalidate(@tenant)

    on_exit(fn ->
      Enum.each(prev, fn {key, value} -> restore_env(key, value) end)
      restore_system_env("JWT_SECRET", prev_secret)
      restore_system_env("AUTH_DISABLED", prev_auth_disabled)
      QueryServiceEx.TenantCache.invalidate(@tenant)
    end)

    :ok
  end

  describe "GET /api/v1/events/query" do
    test "is fail-closed without credentials" do
      conn = request(nil, "?limit=2")

      assert conn.status == 401,
             "an unauthenticated events query must be rejected, got #{conn.status}"

      refute Map.has_key?(Jason.decode!(conn.resp_body), "events")
    end

    test "returns Core's wire envelope including the pagination metadata" do
      conn = request(token(), "?limit=1")

      assert conn.status == 200
      body = Jason.decode!(conn.resp_body)

      assert body["count"] == 1
      assert [%{"id" => "11111111-1111-1111-1111-111111111111"}] = body["events"]

      # #252: the gateway used to rebuild the envelope as `{events, count}` only,
      # so an SDK paginator lost `has_more`/`total_count` and fell back to the
      # "short page means last page" heuristic — wrong for an exact-multiple page.
      assert body["total_count"] == 9
      assert body["has_more"] == true
    end

    test "does not serve the /events/:id show action" do
      # `query` must route to :query_core_compat, not be swallowed by
      # `get("/events/:id")` as an event whose id is the string "query".
      conn = request(token(), "?limit=1")

      assert conn.status == 200
      body = Jason.decode!(conn.resp_body)

      assert Map.has_key?(body, "events"), "must be the query envelope"
      refute Map.has_key?(body, "data"), "`data` means the single-event show action ran"
      refute Map.has_key?(body, "error")
    end

    test "scopes the Core read to the credential's tenant and forwards the filters" do
      request(token(), "?limit=1&event_type_prefix=order.&order=desc")

      forwarded = Agent.get(FakeCoreState, & &1.query)

      assert forwarded["tenant_id"] == @tenant
      assert forwarded["event_type_prefix"] == "order."
      assert forwarded["order"] == "desc"
      assert forwarded["limit"] == "1"
    end
  end

  defp request(token, query_string) do
    :get
    |> conn("/api/v1/events/query" <> query_string, nil)
    |> Plug.Conn.put_req_header("content-type", "application/json")
    # The Endpoint fetches query params before the router in production; calling
    # Router.call/2 directly does not, and without this the controller sees
    # `params == %{}` and every filter assertion below passes vacuously.
    |> Plug.Conn.fetch_query_params()
    |> maybe_authorize(token)
    |> Router.call(@opts)
  end

  defp maybe_authorize(conn, nil), do: conn

  defp maybe_authorize(conn, token),
    do: Plug.Conn.put_req_header(conn, "authorization", "Bearer " <> token)

  defp token do
    claims = %{
      "sub" => "user-1",
      "tenant_id" => @tenant,
      "exp" => System.system_time(:second) + 3600
    }

    jwt_secret()
    |> JOSE.JWK.from_oct()
    |> JOSE.JWT.sign(%{"alg" => "HS256"}, claims)
    |> JOSE.JWS.compact()
    |> elem(1)
  end

  defp jwt_secret, do: "events-query-route-test-secret-key-0123456789"

  defp get_env(key), do: Application.get_env(:query_service_ex, key)

  defp restore_env(key, nil), do: Application.delete_env(:query_service_ex, key)
  defp restore_env(key, value), do: Application.put_env(:query_service_ex, key, value)

  defp restore_system_env(key, nil), do: System.delete_env(key)
  defp restore_system_env(key, value), do: System.put_env(key, value)

  defp free_port do
    {:ok, socket} = :gen_tcp.listen(0, [:binary, active: false, ip: {127, 0, 0, 1}])
    {:ok, port} = :inet.port(socket)
    :ok = :gen_tcp.close(socket)
    port
  end
end
