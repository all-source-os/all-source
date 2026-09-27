defmodule QueryServiceEx.Infrastructure.Adapters.CustomerReviewStoreTest do
  use ExUnit.Case, async: false
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOwner, as: Owner
  alias QueryServiceEx.Domain.CustomerAgent.ReviewWorkspace
  alias QueryServiceEx.Infrastructure.Adapters.CustomerReviewStore, as: Store
  alias QueryServiceEx.TestSupport.AgentRunFixture, as: F
  @tenant "synthetic-review-store"

  defmodule Fixture do
    import Plug.Conn
    def init(options), do: options

    # Ignore unrelated buffered billing flushes through the global Core URL.
    def call(%{request_path: "/api/v1/tenants/" <> _} = conn, _opts),
      do: send_resp(conn, 200, "{}")

    def call(conn, opts) do
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
      for key <- [:core_write_url, :core_api_key],
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

  test "fixed registry key, leader credential and validated config revision" do
    key = "customer_review_v1.workspace." <> Owner.digest(@tenant)
    registry = ReviewWorkspace.empty(@tenant)

    fixture(fn _, _ ->
      {200, Jason.encode!(%{key: key, value: registry, revision: F.uuid(1)})}
    end)

    assert {:ok, ^registry, revision} = Store.load(@tenant)
    assert revision == F.uuid(1)
    path = "/api/v1/config/" <> key
    assert_receive {:request, "GET", ^path, nil, ["Bearer synthetic-admin"]}
    refute_receive {:request, _, _, _, _}
  end

  test "conditional route requires expected revision and stores no credentials" do
    fixture(fn _, input ->
      {200, Jason.encode!(%{key: input["key"], saved: true, revision: F.uuid(2)})}
    end)

    assert :ok = Store.replace(@tenant, ReviewWorkspace.empty(@tenant), F.uuid(1))
    assert_receive {:request, "POST", "/api/v1/config/conditional/set", input, _}
    assert input["condition"] == %{"kind" => "revision", "revision" => F.uuid(1)}
    refute Jason.encode!(input) =~ "synthetic-admin"
    refute_receive {:request, _, _, _, _}
  end

  test "streamed config limit rejects oversized responses before decode" do
    fixture(fn _, _ -> {200, String.duplicate("x", 65_537)} end)
    assert {:error, :storage_unavailable} = Store.load(@tenant)
  end

  test "redirect never forwards Core authority to another route" do
    fixture(fn _, _ -> {307, "SYNTHETIC PRIVATE"} end)
    assert {:error, :storage_unavailable} = Store.load(@tenant)
    assert_receive {:request, _, _, _, _}
    refute_receive {:request, _, _, _, _}
  end

  test "upstream failure is fixed and has no implicit retry" do
    fixture(fn _, _ -> {503, "SYNTHETIC PRIVATE"} end)
    assert {:error, :storage_unavailable} = Store.load(@tenant)
    assert_receive {:request, _, _, _, _}
    refute_receive {:request, _, _, _, _}
  end

  test "unknown conflict response cannot become successful persistence" do
    fixture(fn _, _ -> {409, Jason.encode!(%{error: "SYNTHETIC PRIVATE"})} end)

    assert {:error, :storage_unavailable} =
             Store.replace(@tenant, ReviewWorkspace.empty(@tenant), nil)

    assert_receive {:request, "POST", _, %{"condition" => %{"kind" => "absent"}}, _}
  end

  test "an acknowledgement without a durable revision fails closed" do
    fixture(fn _, input -> {200, Jason.encode!(%{key: input["key"], saved: true})} end)

    assert {:error, :storage_unavailable} =
             Store.replace(@tenant, ReviewWorkspace.empty(@tenant), nil)
  end

  test "invalid tenant, record or marker ID is refused without a network call" do
    fixture(fn _, _ -> {200, "{}"} end)
    assert {:error, :storage_unavailable} = Store.load("?tenant=other")
    assert {:error, :storage_unavailable} = Store.replace(@tenant, %{"approved" => true}, nil)
    assert {:error, :storage_unavailable} = Store.active?(@tenant, "sources", "../../private")

    assert {:error, :storage_unavailable} =
             Store.revoke(@tenant, "secrets", String.duplicate("a", 32))

    refute_receive {:request, _, _, _, _}
  end

  defp fixture(reply) do
    server =
      start_supervised!(
        {Bandit, plug: {Fixture, owner: self(), reply: reply}, ip: {127, 0, 0, 1}, port: 0}
      )

    {:ok, {_ip, port}} = ThousandIsland.listener_info(server)
    Application.put_env(:query_service_ex, :core_write_url, "http://127.0.0.1:#{port}")
  end
end
