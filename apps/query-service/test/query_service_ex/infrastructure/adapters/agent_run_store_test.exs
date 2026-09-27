defmodule QueryServiceEx.Infrastructure.Adapters.AgentRunStoreTest do
  use ExUnit.Case, async: false
  alias QueryServiceEx.Domain.AgentRun.AppendCommand
  alias QueryServiceEx.Domain.AgentRun.Event
  alias QueryServiceEx.Infrastructure.Adapters.AgentRunStore
  alias QueryServiceEx.TestSupport.AgentRunFixture, as: F

  defmodule Fixture do
    import Plug.Conn
    def init(opts), do: opts

    # Global UsageReporter can flush another test's buffered meters while this
    # fixture owns the configured Core URL. These are not run-source requests.
    def call(%{request_path: "/api/v1/tenants/" <> _} = conn, _opts),
      do: send_resp(conn, 200, "{}")

    def call(conn, opts) do
      conn = fetch_query_params(conn)

      conn =
        if conn.method == "POST" do
          {:ok, body, conn} = read_body(conn)
          send(opts[:owner], {:source_append, conn.request_path, Jason.decode!(body)})
          conn
        else
          conn
        end

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
    fixture(200, Jason.encode!(complete_response()))
    assert {:ok, []} = AgentRunStore.events(F.tenant(), F.uuid(1))
    assert_receive {:source_read, "GET", params, ["Bearer synthetic-only"]}

    assert params == %{
             "tenant_id" => F.tenant(),
             "entity_id" => Event.entity(F.tenant(), F.uuid(1)),
             "limit" => "1001",
             "integrity" => "retained-entity-v1"
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
    fixture(200, Jason.encode!(%{complete_response() | total_count: 1, has_more: true}))
    assert {:error, :source_unavailable} = AgentRunStore.events(F.tenant(), F.uuid(1))
  end

  test "a legacy complete-looking response cannot attest retained history" do
    fixture(200, Jason.encode!(%{events: [], count: 0, total_count: 0, has_more: false}))
    assert {:error, :source_unavailable} = AgentRunStore.events(F.tenant(), F.uuid(1))
  end

  test "an attestation for another tenant is refused even when empty" do
    body = put_in(complete_response(), [:archive_integrity, :tenant_id], "synthetic-other")
    fixture(200, Jason.encode!(body))
    assert {:error, :source_unavailable} = AgentRunStore.events(F.tenant(), F.uuid(1))
  end

  test "an attestation for another entity is refused even when empty" do
    body = put_in(complete_response(), [:archive_integrity, :entity_id], "synthetic-other")
    fixture(200, Jason.encode!(body))
    assert {:error, :source_unavailable} = AgentRunStore.events(F.tenant(), F.uuid(1))
  end

  test "an unsupported integrity protocol is refused" do
    body = put_in(complete_response(), [:archive_integrity, :protocol], "retained-entity-v2")
    fixture(200, Jason.encode!(body))
    assert {:error, :source_unavailable} = AgentRunStore.events(F.tenant(), F.uuid(1))
  end

  test "invalid tenant or run is denied before network access" do
    fixture(200, "{}")

    for {tenant, run} <- [{"a&tenant_id=b", F.uuid(1)}, {F.tenant(), "../private"}, {nil, nil}] do
      assert {:error, :source_unavailable} = AgentRunStore.events(tenant, run)
    end

    refute_receive {:source_read, _, _, _}
  end

  test "conditional append uses one bounded fixed endpoint and exact metadata allowlist" do
    ack = %{"event_id" => F.uuid(1001), "version" => 1, "timestamp" => "2026-09-27T09:00:00Z"}
    fixture(200, Jason.encode!(ack))
    request = append_request()
    assert {:ok, ^ack} = AgentRunStore.append(request)
    assert_receive {:source_append, "/api/v1/events", ^request}
    assert_receive {:source_read, "POST", %{}, ["Bearer synthetic-only"]}
    refute_receive {:source_append, _, _}
  end

  test "failed append has no automatic retry and does not echo response contents" do
    fixture(503, "SYNTHETIC PRIVATE")
    assert {:error, :append_uncertain} = AgentRunStore.append(append_request())
    assert_receive {:source_append, _, _}
    refute_receive {:source_append, _, _}
  end

  test "append acknowledgement is bounded before decoding" do
    fixture(200, String.duplicate("x", 4_097))
    assert {:error, :append_uncertain} = AgentRunStore.append(append_request())
    assert_receive {:source_append, _, _}
    refute_receive {:source_append, _, _}
  end

  test "conflict is distinct from uncertain append without exposing upstream error text" do
    fixture(409, "SYNTHETIC PRIVATE")
    assert {:error, :version_conflict} = AgentRunStore.append(append_request())
    assert_receive {:source_append, _, _}
    refute_receive {:source_append, _, _}
  end

  test "wrong acknowledged version cannot become success" do
    fixture(
      200,
      Jason.encode!(%{event_id: F.uuid(1001), version: 2, timestamp: "2026-09-27T09:00:00Z"})
    )

    assert {:error, :append_uncertain} = AgentRunStore.append(append_request())
  end

  test "append refuses forged targets and extra metadata before network access" do
    fixture(200, "{}")
    request = append_request()

    for invalid <- [
          Map.put(request, "entity_id", "different-run"),
          Map.put(request, "tenant_id", "?tenant=other"),
          Map.put(request, "expected_version", nil),
          put_in(request["metadata"]["raw_prompt"], "SYNTHETIC PRIVATE"),
          put_in(request["payload"]["raw_prompt"], "SYNTHETIC PRIVATE")
        ] do
      assert {:error, :append_uncertain} = AgentRunStore.append(invalid)
    end

    refute_receive {:source_append, _, _}
  end

  defp append_request do
    {:ok, command} =
      AppendCommand.new(F.tenant(), F.uuid(1), %{
        "operation_id" => F.uuid(900),
        "expected_version" => 0,
        "event" => F.payload("run.started", F.uuid(1), nil)
      })

    {:append, request} = AppendCommand.prepare(command, [])
    request
  end

  defp complete_response do
    %{
      events: [],
      count: 0,
      total_count: 0,
      has_more: false,
      archive_integrity: %{
        protocol: "retained-entity-v1",
        tenant_id: F.tenant(),
        entity_id: Event.entity(F.tenant(), F.uuid(1))
      }
    }
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
