defmodule QueryServiceEx.Infrastructure.Adapters.RustCoreClientViewsTest do
  @moduledoc """
  The feed, read-model and view-listing calls the view folder makes, asserted on
  the wire: every call names the tenant it acts for, and Core's 409 bodies map
  to typed errors.
  """
  use ExUnit.Case, async: false

  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient

  defmodule FakeCore do
    @moduledoc false
    import Plug.Conn

    def init(opts), do: opts

    def call(conn, _opts) do
      conn = fetch_query_params(conn)
      {:ok, body, conn} = read_body(conn)
      request = %{method: conn.method, path: conn.request_path, query: conn.query_params}
      request = Map.put(request, :body, if(body == "", do: nil, else: Jason.decode!(body)))
      :ok = Agent.update(ViewsFakeCoreState, &[request | &1])
      {status, response} = respond(request)

      conn
      |> put_resp_content_type("application/json")
      |> send_resp(status, Jason.encode!(response))
    end

    defp respond(%{path: "/api/v1/event-feed", query: %{"epoch" => "40"}}) do
      {409, %{"error" => "version_conflict", "expected_version" => 40, "current_version" => 41}}
    end

    defp respond(%{path: "/api/v1/event-feed"}) do
      {200,
       %{
         "tenant_id" => "t1",
         "cursor" => %{"epoch" => 41, "position" => 3},
         "head" => %{"epoch" => 41, "position" => 3},
         "events" => [],
         "positions" => [],
         "caught_up" => true
       }}
    end

    defp respond(%{method: "PUT", body: %{"expected_revision" => 6}}) do
      {409, %{"error" => "version_conflict", "expected_version" => 6, "current_version" => 7}}
    end

    defp respond(%{method: "PUT"}), do: {200, %{"revision" => 8}}

    defp respond(%{path: "/api/v1/read-models/" <> _name}) do
      {200, %{"revision" => 7, "checkpoint" => nil, "ready" => false, "rows" => %{}}}
    end

    defp respond(%{path: "/api/v1/admin/views", query: %{"after" => "t1/a"}}) do
      {200, %{"views" => [%{"tenant_id" => "t2", "view" => "b"}], "next" => nil}}
    end

    defp respond(%{path: "/api/v1/admin/views"}) do
      {200, %{"views" => [%{"tenant_id" => "t1", "view" => "a"}], "next" => "t1/a"}}
    end

    defp respond(_request), do: {404, %{"error" => "not_found"}}
  end

  setup do
    start_supervised!(%{
      id: ViewsFakeCoreState,
      start: {Agent, :start_link, [fn -> [] end, [name: ViewsFakeCoreState]]}
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

  describe "event_feed/4" do
    test "sends the tenant, cursor and joined prefixes" do
      cursor = %{"epoch" => 41, "position" => 2}

      assert {:ok, %{"caught_up" => true}} =
               RustCoreClient.event_feed("t1", cursor, ["ops.", "skill."], 50)

      assert [%{query: query}] = requests()

      assert query == %{
               "tenant_id" => "t1",
               "epoch" => "41",
               "position" => "2",
               "prefixes" => "ops.,skill.",
               "limit" => "50"
             }
    end

    test "omits the cursor and prefixes when there are none" do
      assert {:ok, _page} = RustCoreClient.event_feed("t1", nil, [], 10)
      assert [%{query: %{"tenant_id" => "t1", "limit" => "10"} = query}] = requests()
      refute Map.has_key?(query, "epoch")
      refute Map.has_key?(query, "prefixes")
    end

    test "maps a 409 to an epoch change carrying the current epoch" do
      cursor = %{"epoch" => 40, "position" => 17}

      assert {:error, {:epoch_changed, 41}} = RustCoreClient.event_feed("t1", cursor, [], 10)
    end
  end

  describe "read_model_page/3" do
    test "acts for the tenant and forwards the paging options" do
      assert {:ok, %{"revision" => 7}} =
               RustCoreClient.read_model_page("t1", "view_ops", prefix: "a:", limit: 1)

      assert [%{method: "GET", path: "/api/v1/read-models/view_ops", query: query}] = requests()
      assert query == %{"tenant_id" => "t1", "prefix" => "a:", "limit" => "1"}
    end
  end

  describe "commit_read_model/3" do
    test "puts the commit for the tenant and returns the new revision" do
      commit = %{
        expected_revision: 7,
        checkpoint: %{"epoch" => 41, "position" => 3},
        ready: true,
        reset: false,
        changes: %{"a" => %{"n" => 1}, "b" => nil}
      }

      assert {:ok, 8} = RustCoreClient.commit_read_model("t1", "view_ops", commit)
      assert [%{method: "PUT", query: %{"tenant_id" => "t1"}, body: body}] = requests()
      assert body["changes"] == %{"a" => %{"n" => 1}, "b" => nil}
      assert body["expected_revision"] == 7
    end

    test "maps a 409 to a version conflict carrying the current revision" do
      commit = %{expected_revision: 6, checkpoint: nil, ready: true, reset: false, changes: %{}}

      assert {:error, {:version_conflict, 7}} =
               RustCoreClient.commit_read_model("t1", "view_ops", commit)
    end
  end

  describe "list_all_views/0" do
    test "follows next until the listing ends" do
      assert {:ok, views} = RustCoreClient.list_all_views()
      assert Enum.map(views, & &1["view"]) == ["a", "b"]
      assert Enum.all?(requests(), &(&1.path == "/api/v1/admin/views"))
    end
  end

  defp requests, do: ViewsFakeCoreState |> Agent.get(& &1) |> Enum.reverse()

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
