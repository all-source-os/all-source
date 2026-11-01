defmodule QueryServiceEx.Infrastructure.Adapters.CoreHoldTest do
  @moduledoc """
  The Core client across a Core restart, asserted on the wire: a request is held
  while Core refuses the dial or answers not_ready, and a write Core may already
  have read is never sent twice.
  """
  use ExUnit.Case, async: false

  alias QueryServiceEx.Infrastructure.Adapters.CoreHold
  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient

  defmodule FakeCore do
    @moduledoc false
    import Plug.Conn

    def init(opts), do: opts

    def call(conn, _opts) do
      {:ok, body, conn} = read_body(conn)
      hit = %{method: conn.method, path: conn.request_path, body: body}

      {status, response} =
        Agent.get_and_update(HoldFakeCoreState, fn %{script: script, hits: hits} ->
          {next, rest} = next_response(script, conn.method)
          {next, %{script: rest, hits: [hit | hits]}}
        end)

      conn
      |> put_resp_content_type("application/json")
      |> send_resp(status, Jason.encode!(response))
    end

    defp next_response([next | rest], _method), do: {next, rest}
    defp next_response([], "GET"), do: {{200, %{"id" => "t1", "status" => "active"}}, []}
    defp next_response([], _method), do: {{201, %{"event_id" => "evt-1"}}, []}
  end

  @event %{entity_id: "order-1", event_type: "order.placed", payload: %{"n" => 1}}

  setup do
    start_supervised!(%{
      id: HoldFakeCoreState,
      start: {Agent, :start_link, [fn -> %{script: [], hits: []} end, [name: HoldFakeCoreState]]}
    })

    port = free_port()
    point_core_at("http://127.0.0.1:#{port}")

    {:ok, port: port}
  end

  describe "while Core refuses the dial" do
    test "a write is held until Core is up and reaches Core once", %{port: port} do
      write = Task.async(fn -> RustCoreClient.create_event("t1", @event) end)
      Process.sleep(300)
      start_core(port)

      assert {:ok, %{"event_id" => "evt-1"}} = Task.await(write, 10_000)
      assert [%{method: "POST", path: "/api/v1/events"}] = hits("/api/v1/events")
    end

    test "the leader authorization read rides the outage instead of failing", %{port: port} do
      read = Task.async(fn -> RustCoreClient.get_tenant_for_authorization("t1") end)
      Process.sleep(300)
      start_core(port)

      assert {:ok, %{"id" => "t1"}} = Task.await(read, 10_000)
      assert [%{method: "GET", path: "/api/v1/tenants/t1"}] = hits("/api/v1/tenants/t1")
    end
  end

  describe "when Core answers 503" do
    test "not_ready is retried and the write lands", %{port: port} do
      script([{503, %{"error" => "not_ready"}}])
      start_core(port)

      assert {:ok, %{"event_id" => "evt-1"}} = RustCoreClient.create_event("t1", @event)
      assert [%{method: "POST"}, %{method: "POST"}] = hits("/api/v1/events")
    end

    test "any other 503 on a write is returned without a resend", %{port: port} do
      script([{503, %{"error" => "overloaded"}}])
      start_core(port)

      assert {:error, "HTTP 503: " <> _} = RustCoreClient.create_event("t1", @event)
      assert [%{method: "POST"}] = hits("/api/v1/events")
    end
  end

  describe "when Core resets the connection after reading the request" do
    test "a write is not resent", %{port: port} do
      start_resetting_core(port)

      assert {:error, _reason} = RustCoreClient.create_event("t1", @event)
      assert_received {:core_read, "POST /api/v1/events " <> _}
      refute_receive {:core_read, "POST /api/v1/events " <> _}, 300
    end

    test "a read is resent up to max_retries", %{port: port} do
      start_resetting_core(port)

      client =
        Tesla.client(
          [
            {Tesla.Middleware.BaseUrl, "http://127.0.0.1:#{port}"},
            {CoreHold, max_retries: 2},
            Tesla.Middleware.JSON
          ],
          Tesla.Adapter.Hackney
        )

      assert {:error, _reason} = Tesla.get(client, "/api/v1/tenants/t1")

      for _attempt <- 1..3, do: assert_received({:core_read, "GET /api/v1/tenants/t1 " <> _})
      refute_receive {:core_read, "GET /api/v1/tenants/t1 " <> _}, 300
    end
  end

  describe "stores that call Core through Req" do
    test "a write is held through a refused dial and reaches Core once", %{port: port} do
      write =
        Task.async(fn ->
          Req.post(
            "http://127.0.0.1:#{port}/api/v1/events",
            [json: @event, receive_timeout: 2_000] ++ CoreHold.req_options()
          )
        end)

      Process.sleep(300)
      start_core(port)

      assert {:ok, %Req.Response{status: 201}} = Task.await(write, 10_000)
      assert [%{method: "POST", path: "/api/v1/events"}] = hits("/api/v1/events")
    end

    test "a write is not resent after a reset", %{port: port} do
      start_resetting_core(port)

      assert {:error, _reason} =
               Req.post(
                 "http://127.0.0.1:#{port}/api/v1/events",
                 [json: @event, receive_timeout: 2_000] ++ CoreHold.req_options()
               )

      assert_received {:core_read, "POST /api/v1/events " <> _}
      refute_receive {:core_read, "POST /api/v1/events " <> _}, 300
    end
  end

  describe "API key verification while Core gives no answer" do
    @secret "test_jwt_secret_for_testing_purposes_only_at_least_32_bytes"

    setup do
      original_secret = System.get_env("JWT_SECRET")
      original_mode = Application.get_env(:query_service_ex, :key_authority_mode)
      System.put_env("JWT_SECRET", @secret)
      Application.put_env(:query_service_ex, :key_authority_mode, :off)

      on_exit(fn ->
        if original_secret,
          do: System.put_env("JWT_SECRET", original_secret),
          else: System.delete_env("JWT_SECRET")

        restore_env(:key_authority_mode, original_mode)
      end)
    end

    test "is reported as Core unavailable, not as an invalid key" do
      now = System.system_time(:second)

      claims = %{
        "sub" => "key-1",
        "tenant_id" => "t1",
        "role" => "serviceaccount",
        "is_api_key" => true,
        "iat" => now,
        "exp" => now + 3600
      }

      {_alg, key} =
        JOSE.JWK.from_oct(@secret)
        |> JOSE.JWT.sign(%{"alg" => "HS256"}, JOSE.JWT.from_map(claims))
        |> JOSE.JWS.compact()

      assert {:error, :core_unavailable} = RustCoreClient.verify_api_key(key)
    end
  end

  defp start_core(port) do
    start_supervised!({Bandit, plug: FakeCore, scheme: :http, port: port, ip: {127, 0, 0, 1}})
  end

  defp start_resetting_core(port) do
    {:ok, listen} =
      :gen_tcp.listen(port, [:binary, active: false, reuseaddr: true, ip: {127, 0, 0, 1}])

    test = self()
    spawn_link(fn -> accept_and_reset(listen, test) end)
  end

  defp accept_and_reset(listen, test) do
    case :gen_tcp.accept(listen) do
      {:ok, socket} ->
        send(test, {:core_read, read_request(socket, "")})
        :ok = :inet.setopts(socket, linger: {true, 0})
        :gen_tcp.close(socket)
        accept_and_reset(listen, test)

      {:error, _reason} ->
        :ok
    end
  end

  defp read_request(socket, acc) do
    {:ok, data} = :gen_tcp.recv(socket, 0, 2_000)
    acc = acc <> data

    case :binary.split(acc, "\r\n\r\n") do
      [head, body] ->
        if byte_size(body) >= content_length(head), do: acc, else: read_request(socket, acc)

      [_partial] ->
        read_request(socket, acc)
    end
  end

  defp content_length(head) do
    case Regex.run(~r/content-length:\s*(\d+)/i, head) do
      [_, length] -> String.to_integer(length)
      nil -> 0
    end
  end

  defp script(responses), do: Agent.update(HoldFakeCoreState, &%{&1 | script: responses})

  # Background workers share the Core URL, so assertions filter to the path under test.
  defp hits(path) do
    HoldFakeCoreState |> Agent.get(& &1.hits) |> Enum.reverse() |> Enum.filter(&(&1.path == path))
  end

  defp point_core_at(url) do
    keys = [:core_url, :core_read_urls, :core_write_url]
    prev = Enum.map(keys, &{&1, Application.get_env(:query_service_ex, &1)})

    Application.put_env(:query_service_ex, :core_url, url)
    Application.put_env(:query_service_ex, :core_read_urls, [url])
    Application.put_env(:query_service_ex, :core_write_url, url)

    on_exit(fn -> Enum.each(prev, fn {key, value} -> restore_env(key, value) end) end)
  end

  defp restore_env(key, nil), do: Application.delete_env(:query_service_ex, key)
  defp restore_env(key, value), do: Application.put_env(:query_service_ex, key, value)

  defp free_port do
    {:ok, socket} = :gen_tcp.listen(0, [:binary, active: false, ip: {127, 0, 0, 1}])
    {:ok, port} = :inet.port(socket)
    :ok = :gen_tcp.close(socket)
    port
  end
end
