defmodule QueryServiceEx.Integration.CustomerReviewWorkTest do
  use ExUnit.Case, async: false
  alias QueryServiceEx.Application.Services.CustomerEvidenceSources, as: Sources
  alias QueryServiceEx.Application.Services.CustomerReviewDeadline, as: Deadline
  alias QueryServiceEx.Infrastructure.Adapters.AgentRunStore
  alias QueryServiceEx.Infrastructure.Adapters.CustomerQueryUsageStore, as: Usage
  alias QueryServiceEx.Infrastructure.Adapters.CustomerReviewStore, as: Records
  alias QueryServiceEx.TestSupport.MeteredEvidenceFixture, as: F
  import QueryServiceEx.TestSupport.CustomerAgentCore, only: [with_core: 2]
  @moduletag :integration
  @moduletag timeout: 60_000
  @moduletag skip: is_nil(System.get_env("ALLSOURCE_CORE_BINARY"))

  defmodule PausedSource do
    def events(tenant, run) do
      result = AgentRunStore.events(tenant, run)
      send(Application.fetch_env!(:query_service_ex, :meter_test_owner), {:read_paused, self()})

      receive do
        :resume -> result
      after
        5_000 -> {:error, :source_unavailable}
      end
    end
  end

  setup do
    F.setup_context()
  end

  test "busy admission neither charges nor reads a source, and the original request retries",
       context do
    with_core(context, fn ->
      now = System.system_time(:second)
      grant = F.provision(1, now)
      input = F.source_input(1, now)
      F.observe_reads()
      parent = self()

      holders =
        for _ <- 1..2 do
          caller =
            Task.async(fn ->
              Deadline.run(F.actor(), fn ->
                send(parent, {:holding, self()})

                receive do
                  :release -> :released
                after
                  5_000 -> :fixture_timeout
                end
              end)
            end)

          assert_receive {:holding, worker}, 1_000
          {caller, worker}
        end

      assert {:error, :review_busy} = Sources.share(F.actor(), grant.id, input, now)
      refute_receive {:evidence_read, _}
      assert {:ok, %{"used" => 0}} = Usage.snapshot(F.tenant())

      Enum.each(holders, fn {caller, worker} ->
        send(worker, :release)
        assert :released = Task.await(caller, 1_000)
      end)

      assert {:ok, _} = Sources.share(F.actor(), grant.id, input, now)
      assert_receive {:evidence_read, _}
      assert {:ok, %{"used" => 1}} = Usage.snapshot(F.tenant())
    end)
  end

  test "caller cancellation after real source read cannot persist a late source or charge twice",
       context do
    with_core(context, fn ->
      now = System.system_time(:second)
      grant = F.provision(1, now)
      input = F.source_input(1, now)
      Application.put_env(:query_service_ex, :agent_run_source, PausedSource)
      caller = spawn(fn -> Sources.share(F.actor(), grant.id, input, now) end)
      assert_receive {:read_paused, worker}, 5_000
      monitor = Process.monitor(worker)
      assert {:ok, %{"used" => 1}} = Usage.snapshot(F.tenant())
      Process.exit(caller, :kill)
      assert_receive {:DOWN, ^monitor, :process, ^worker, :killed}, 1_000
      send(worker, :resume)
      assert {:ok, workspace, _} = Records.load(F.tenant())
      assert workspace["sources"] == %{}
      F.observe_reads()
      assert {:ok, _} = Sources.share(F.actor(), grant.id, input, now)
      assert_receive {:evidence_read, _}
      assert {:ok, %{"used" => 1}} = Usage.snapshot(F.tenant())
      assert {:ok, workspace, _} = Records.load(F.tenant())
      assert map_size(workspace["sources"]) == 1
    end)
  end
end
