defmodule QueryServiceEx.Integration.TrackedReplaysTest do
  use ExUnit.Case, async: false
  alias QueryServiceEx.Infrastructure.Adapters.ReplayJournal, as: Journal
  alias QueryServiceEx.Projections.Catalog
  alias QueryServiceEx.Projections.TenantProjections, as: Engine
  alias QueryServiceEx.Projections.TrackedReplays, as: Replays
  alias QueryServiceEx.TestSupport.AgentRunFixture, as: F
  alias QueryServiceEx.TestSupport.CustomerAgentCore, as: Core

  @moduletag :integration
  @moduletag timeout: 120_000
  @moduletag skip: is_nil(System.get_env("ALLSOURCE_CORE_BINARY"))
  @tenant "synthetic-tracked-replay"

  defmodule LostAckProxy do
    import Plug.Conn
    def init(options), do: options

    def call(conn, opts) do
      {:ok, body, conn} = read_body(conn)

      response =
        Req.request!(
          method: if(conn.method == "GET", do: :get, else: :post),
          url: opts[:url] <> conn.request_path,
          headers: [{"authorization", opts[:token]}, {"content-type", "application/json"}],
          body: body,
          retry: false,
          redirect: false,
          receive_timeout: 5_000
        )

      input = if body == "", do: %{}, else: Jason.decode!(body)
      condition = get_in(input, ["condition", "kind"])

      # Core commits the actual write, but its durable acknowledgement is lost.
      output =
        if response.status == 200 and condition == opts[:condition],
          do: %{"saved" => true, "key" => input["key"]},
          else: response.body

      conn
      |> put_resp_content_type("application/json")
      |> send_resp(response.status, Jason.encode!(output))
    end
  end

  setup do
    context = Core.setup_context()
    prior = Application.get_env(:query_service_ex, :tenant_projection_query_fun)
    Engine.init_tables()

    if Process.whereis(QueryServiceEx.Projections.BackfillSupervisor) == nil,
      do:
        start_supervised!({Task.Supervisor, name: QueryServiceEx.Projections.BackfillSupervisor})

    if Process.whereis(Engine) == nil, do: start_supervised!(Engine)
    seed(1)

    on_exit(fn ->
      Engine.disable(@tenant, "event-count")
      Engine.disable(@tenant, "entity-activity")

      if prior,
        do: Application.put_env(:query_service_ex, :tenant_projection_query_fun, prior),
        else: Application.delete_env(:query_service_ex, :tenant_projection_query_fun)
    end)

    context
  end

  test "eight competing starts dispatch once; result and identity survive both restarts",
       context do
    operation = F.uuid(901)

    completed =
      Core.with_core(context, fn ->
        enable()
        {counter, gate} = blocked_source(3)

        replies =
          1..8
          |> Task.async_stream(fn _ -> Replays.start(@tenant, "event-count", operation) end,
            max_concurrency: 8,
            timeout: 20_000
          )
          |> Enum.map(fn {:ok, {:ok, result}} -> result end)

        assert length(Enum.uniq_by(replies, & &1["replay_id"])) == 1
        assert :atomics.get(counter, 1) == 1
        assert Enum.all?(replies, &(&1["status"] in ~w(unknown running)))
        assert {:ok, %{"total" => 1}} = state()
        :atomics.put(gate, 1, 1)
        completed = terminal(operation, "completed")
        assert completed["processed_events"] == 3
        assert {:ok, %{"total" => 3}} = state()
        assert :atomics.get(counter, 1) == 1
        assert {:ok, ^completed} = Replays.start(@tenant, "event-count", operation)

        assert {:error, :operation_conflict} =
                 Replays.start(@tenant, "entity-activity", operation)

        assert {:error, :not_found} = Replays.get("another-tenant", operation)
        assert {:ok, record, _} = Journal.load(@tenant, operation)

        assert {:error, :conflict} =
                 Journal.finish(record, %{
                   "status" => "failed",
                   "completed_at" => completed["completed_at"],
                   "processed_events" => 0
                 })

        assert :atomics.get(counter, 1) == 1
        completed
      end)

    restart_engine()

    Core.with_core(context, fn ->
      assert {:ok, ^completed} = Replays.get(@tenant, operation)
      assert {:ok, ^completed} = Replays.start(@tenant, "event-count", operation)
      assert Engine.list_replays(@tenant) == []
    end)
  end

  test "lost reservation acknowledgement cannot dispatch, including after Core restart",
       context do
    operation = F.uuid(902)

    pending =
      Core.with_core(context, fn ->
        enable()
        {counter, gate} = blocked_source(2)
        :atomics.put(gate, 1, 1)
        proxy(context, "absent")

        assert {:error, :storage_unavailable} = Replays.start(@tenant, "event-count", operation)
        assert {:ok, pending} = Replays.start(@tenant, "event-count", operation)
        assert pending["status"] == "unknown"
        assert :atomics.get(counter, 1) == 0
        assert {:ok, %{"total" => 1}} = state()
        pending
      end)

    Application.put_env(:query_service_ex, :core_write_url, context.url)

    Core.with_core(context, fn ->
      assert {:ok, ^pending} = Replays.start(@tenant, "event-count", operation)
    end)
  end

  test "lost completion acknowledgement recovers recorded result without another fold", context do
    Core.with_core(context, fn ->
      enable()
      {counter, gate} = blocked_source(2)
      proxy(context, "revision")
      operation = F.uuid(903)
      assert {:ok, _} = Replays.start(@tenant, "event-count", operation)
      :atomics.put(gate, 1, 1)
      completed = terminal(operation, "completed")
      assert {:ok, ^completed} = Replays.start(@tenant, "event-count", operation)
      assert :atomics.get(counter, 1) == 1
      assert {:ok, %{"total" => 2}} = state()
    end)
  end

  test "Query Service restart during a fold keeps unknown identity and cannot publish old worker",
       context do
    Core.with_core(context, fn ->
      enable()
      {counter, gate} = blocked_source(4)
      operation = F.uuid(904)
      assert {:ok, started} = Replays.start(@tenant, "event-count", operation)
      assert_receive {:fold_started, worker}, 5_000
      monitor = Process.monitor(worker)
      restart_engine()
      assert {:ok, unknown} = Replays.get(@tenant, operation)
      assert unknown["status"] == "unknown"
      assert unknown["replay_id"] == started["replay_id"]
      :atomics.put(gate, 1, 1)
      assert_receive {:DOWN, ^monitor, :process, ^worker, _}, 5_000
      assert {:ok, ^unknown} = Replays.start(@tenant, "event-count", operation)
      assert :atomics.get(counter, 1) == 1
      refute match?({:ok, %{"total" => 4}}, state())
    end)
  end

  test "cancel and disable retain old generation and persist terminal receipt", context do
    Core.with_core(context, fn ->
      enable()

      for {operation, action} <- [{F.uuid(905), :cancel}, {F.uuid(906), :disable}] do
        {_counter, gate} = blocked_source(9)
        assert {:ok, started} = Replays.start(@tenant, "event-count", operation)
        assert_receive {:fold_started, worker}, 5_000
        monitor = Process.monitor(worker)

        if action == :cancel do
          assert {:ok, %{status: "cancelled"}} =
                   Engine.cancel_replay(@tenant, started["replay_id"])

          assert {:ok, %{"total" => 1}} = state()
        else
          assert :ok = Engine.disable(@tenant, "event-count")
          assert {:error, :not_found} = state()
        end

        cancelled = terminal(operation, "cancelled")
        :atomics.put(gate, 1, 1)
        assert_receive {:DOWN, ^monitor, :process, ^worker, _}, 5_000
        assert {:ok, ^cancelled} = Replays.start(@tenant, "event-count", operation)
        refute match?({:ok, %{"total" => 9}}, state())
      end
    end)
  end

  test "Core outage after publication recovers from existing result without redispatch",
       context do
    operation = F.uuid(907)

    {started, counter, gate} =
      Core.with_core(context, fn ->
        enable()
        {counter, gate} = blocked_source(5)
        assert {:ok, started} = Replays.start(@tenant, "event-count", operation)
        {started, counter, gate}
      end)

    :atomics.put(gate, 1, 1)

    wait_until(fn ->
      match?({:ok, %{status: "completed"}}, Engine.get_replay(@tenant, started["replay_id"]))
    end)

    assert {:ok, %{"total" => 5}} = state()
    assert {:error, :storage_unavailable} = Replays.get(@tenant, operation)

    Core.with_core(context, fn ->
      completed = terminal(operation, "completed")
      assert completed["replay_id"] == started["replay_id"]
      assert {:ok, ^completed} = Replays.start(@tenant, "event-count", operation)
      assert :atomics.get(counter, 1) == 1
    end)
  end

  test "source failure preserves accepted state and cannot leak exception text into receipt",
       context do
    Core.with_core(context, fn ->
      enable()

      Application.put_env(:query_service_ex, :tenant_projection_query_fun, fn _, _ ->
        {:error, "synthetic-private-source-instruction"}
      end)

      operation = F.uuid(908)
      assert {:ok, _} = Replays.start(@tenant, "event-count", operation)
      failed = terminal(operation, "failed")
      assert {:ok, %{"total" => 1}} = state()
      refute inspect(failed) =~ "synthetic-private"
      assert {:ok, record, _} = Journal.load(@tenant, operation)
      refute inspect(record) =~ "synthetic-private"
      assert {:ok, ^failed} = Replays.start(@tenant, "event-count", operation)
    end)
  end

  defp enable do
    :ok = Engine.enable(@tenant, "event-count")
    wait_until(fn -> Engine.status(@tenant, "event-count") == :ready end)
  end

  defp seed(count) do
    Application.put_env(:query_service_ex, :tenant_projection_query_fun, fn tenant, _ ->
      {:ok, events(tenant, count)}
    end)
  end

  defp blocked_source(count) do
    owner = self()
    counter = :atomics.new(1, [])
    gate = :atomics.new(1, [])

    Application.put_env(:query_service_ex, :tenant_projection_query_fun, fn tenant, _ ->
      :atomics.add(counter, 1, 1)
      send(owner, {:fold_started, self()})
      wait_until(fn -> :atomics.get(gate, 1) == 1 end)
      {:ok, events(tenant, count)}
    end)

    {counter, gate}
  end

  defp events(tenant, count),
    do:
      for(
        index <- 1..count,
        do: %{"tenant_id" => tenant, "entity_id" => "e#{index}", "event_type" => "created"}
      )

  defp state, do: Engine.get_state(@tenant, "event-count", Catalog.tenant_key())

  defp terminal(operation, status) do
    wait_until(fn -> match?({:ok, %{"status" => ^status}}, Replays.get(@tenant, operation)) end)
    {:ok, result} = Replays.get(@tenant, operation)
    result
  end

  defp restart_engine do
    parent =
      if Enum.any?(Supervisor.which_children(QueryServiceEx.Supervisor), fn {id, _, _, _} ->
           id == Engine
         end),
         do: QueryServiceEx.Supervisor,
         else: elem(ExUnit.fetch_test_supervisor(), 1)

    :ok = Supervisor.terminate_child(parent, Engine)
    {:ok, _} = Supervisor.restart_child(parent, Engine)
  end

  defp proxy(context, condition) do
    server =
      start_supervised!(
        {Bandit,
         plug:
           {LostAckProxy,
            url: context.url,
            token: Application.fetch_env!(:query_service_ex, :core_api_key),
            condition: condition},
         ip: {127, 0, 0, 1},
         port: 0}
      )

    {:ok, {_ip, port}} = ThousandIsland.listener_info(server)
    Application.put_env(:query_service_ex, :core_write_url, "http://127.0.0.1:#{port}")
  end

  defp wait_until(fun, remaining \\ 500)
  defp wait_until(_fun, 0), do: flunk("bounded replay observation expired")

  defp wait_until(fun, remaining) do
    unless fun.() do
      # test-hang-allow: bounded synthetic fold/recovery observation, at most 5 seconds.
      Process.sleep(10)
      wait_until(fun, remaining - 1)
    end
  end
end
