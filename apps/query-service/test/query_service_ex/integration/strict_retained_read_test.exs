defmodule QueryServiceEx.Integration.StrictRetainedReadTest do
  use ExUnit.Case, async: false
  alias QueryServiceEx.Domain.AgentRun.Event
  alias QueryServiceEx.Infrastructure.Adapters.AgentRunStore
  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient
  alias QueryServiceEx.TestSupport.AgentRunFixture, as: F
  alias QueryServiceEx.TestSupport.CustomerAgentCore
  import QueryServiceEx.TestSupport.CustomerAgentCore, only: [with_core: 2]

  @moduletag :integration
  @moduletag timeout: 60_000
  @moduletag skip: is_nil(System.get_env("ALLSOURCE_CORE_BINARY"))

  setup do
    CustomerAgentCore.setup_context()
  end

  test "real HTTP binds strict empty and retained histories through restart", context do
    with_core(context, fn ->
      assert {:ok, []} = AgentRunStore.events(F.tenant(), F.uuid(1))
      assert {:ok, %{status: 200}} = append()
      assert_history()
    end)

    with_core(context, fn ->
      assert_history()
      assert {:ok, []} = AgentRunStore.events(F.tenant(), F.uuid(2))
      assert {:ok, %{"events" => [_event]} = legacy} = legacy_query()
      refute Map.has_key?(legacy, "archive_integrity")
    end)
  end

  test "tolerant reads and compaction cannot erase damaged archive evidence", context do
    with_core(context, fn ->
      assert {:ok, %{status: 200}} = append()
    end)

    partition = Path.join([context.directory, "storage", F.tenant(), "2026-09"])
    File.mkdir_p!(partition)
    corrupt = Path.join(partition, "events-unreadable.parquet")
    File.write!(corrupt, "synthetic unreadable retained history")
    File.write!(Path.join(partition, "events-unreadable-second.parquet"), "synthetic unreadable")

    for _restart <- 1..2 do
      with_core(context, fn ->
        assert {:error, :source_unavailable} = AgentRunStore.events(F.tenant(), F.uuid(1))
        assert {:ok, %{"events" => [_event]}} = legacy_query()
        originals = retained_files(context)
        assert map_size(originals) >= 3

        assert {:ok, %{status: 200}} =
                 Tesla.post(RustCoreClient.write_client(), "/api/v1/compaction/trigger", %{})

        assert retained_files(context) == originals
        assert {:error, :source_unavailable} = AgentRunStore.events(F.tenant(), F.uuid(1))
        assert {:error, :source_unavailable} = AgentRunStore.events(F.tenant(), F.uuid(2))
        assert {:ok, %{status: 200}} = Tesla.get(RustCoreClient.write_client(), "/health")
      end)
    end

    assert File.read!(corrupt) == "synthetic unreadable retained history"
  end

  defp retained_files(context) do
    context.directory
    |> Path.join("storage/#{F.tenant()}/**/*.parquet")
    |> Path.wildcard()
    |> Map.new(&{&1, File.read!(&1)})
  end

  defp assert_history do
    assert {:ok, [event]} = AgentRunStore.events(F.tenant(), F.uuid(1))
    assert event["tenant_id"] == F.tenant()
    assert event["entity_id"] == Event.entity(F.tenant(), F.uuid(1))
    assert event["payload"] == F.payload("run.started", F.uuid(1), nil)
  end

  defp append do
    Tesla.post(RustCoreClient.write_client(), "/api/v1/events", %{
      tenant_id: F.tenant(),
      entity_id: Event.entity(F.tenant(), F.uuid(1)),
      event_type: "agent_run.v1.run.started",
      payload: F.payload("run.started", F.uuid(1), nil),
      expected_version: 0
    })
  end

  defp legacy_query,
    do:
      RustCoreClient.query_events_page(
        F.tenant(),
        %{entity_id: Event.entity(F.tenant(), F.uuid(1)), limit: 1001},
        consistency: :strong
      )
end
