defmodule QueryServiceEx.Integration.AgentRunRecorderTest do
  use ExUnit.Case, async: false
  alias QueryServiceEx.Application.Services.AgentRunEvidence, as: Evidence
  alias QueryServiceEx.Application.Services.AgentRunRecorder, as: Recorder
  alias QueryServiceEx.Infrastructure.Adapters.AgentRunStore
  alias QueryServiceEx.TestSupport.AgentRunFixture, as: F
  alias QueryServiceEx.TestSupport.CustomerAgentCore
  import QueryServiceEx.TestSupport.CustomerAgentCore, only: [with_core: 2]
  @moduletag :integration
  @moduletag timeout: 60_000
  @moduletag skip: is_nil(System.get_env("ALLSOURCE_CORE_BINARY"))

  defmodule LostAcknowledgement do
    def append(request) do
      send(Application.fetch_env!(:query_service_ex, :run_test_owner), :append_attempted)
      {:ok, _} = AgentRunStore.append(request)
      {:error, :append_uncertain}
    end
  end

  setup do
    keys = [:agent_run_writer, :run_test_owner]
    previous = Enum.map(keys, &{&1, Application.get_env(:query_service_ex, &1)})

    on_exit(fn ->
      Enum.each(previous, fn
        {key, nil} -> Application.delete_env(:query_service_ex, key)
        {key, value} -> Application.put_env(:query_service_ex, key, value)
      end)
    end)

    CustomerAgentCore.setup_context()
  end

  test "typed run capture and exact retries survive a hard restart", context do
    {commands, receipts, run} =
      with_core(context, fn ->
        {commands, receipts} = capture_history()
        assert {:ok, run} = Evidence.read(F.tenant(), F.uuid(1))
        assert run.revision == 7
        assert run.completed
        assert run.unknowns == []
        {commands, receipts, run}
      end)

    with_core(context, fn ->
      for {input, original} <- Enum.zip(commands, receipts) do
        assert {:ok, recovered} = record(input)
        assert recovered == %{original | disposition: "already_recorded"}
      end

      assert {:ok, ^run} = Evidence.read(F.tenant(), F.uuid(1))
      changed = put_in(hd(commands)["event"]["prompt_sha256"], F.hash(50))
      assert {:error, :operation_conflict} = record(changed)
      assert {:ok, ^run} = Evidence.read(F.tenant(), F.uuid(1))
    end)
  end

  test "simultaneous identical commands persist one event", context do
    with_core(context, fn ->
      input = command(F.payload("run.started", F.uuid(1), nil), 0)
      tasks = for _ <- 1..8, do: Task.async(fn -> record(input) end)
      results = Task.await_many(tasks, 20_000)
      assert Enum.all?(results, &match?({:ok, _}, &1))
      receipts = Enum.map(results, fn {:ok, receipt} -> receipt end)
      assert Enum.count(receipts, &(&1.disposition == "recorded")) == 1
      assert receipts |> Enum.map(& &1.event_id) |> Enum.uniq() |> length() == 1
      assert {:ok, run} = Evidence.read(F.tenant(), F.uuid(1))
      assert run.revision == 1
    end)
  end

  test "competing different commands cannot both claim the same revision", context do
    with_core(context, fn ->
      assert {:ok, first} = record(command(F.payload("run.started", F.uuid(1), nil), 0))
      proposal = command(F.payload("change.proposed", F.uuid(1), first.event_id), 1)

      gap =
        command(F.payload("capture_gap", F.uuid(1), first.event_id), 1)
        |> Map.put("operation_id", F.uuid(999))

      results =
        [Task.async(fn -> record(proposal) end), Task.async(fn -> record(gap) end)]
        |> Task.await_many(20_000)

      assert Enum.count(results, &match?({:ok, %{disposition: "recorded"}}, &1)) == 1
      assert Enum.count(results, &(&1 == {:error, :stale_revision})) == 1
      assert {:ok, run} = Evidence.read(F.tenant(), F.uuid(1))
      assert run.revision == 2
    end)
  end

  test "lost start acknowledgement recovers evidence without starting another attempt", context do
    input =
      with_core(context, fn ->
        {_commands, receipts} = capture_history(3)
        previous = List.last(receipts).event_id
        input = command(F.payload("attempt.started", F.uuid(1), previous), 3)
        Application.put_env(:query_service_ex, :agent_run_writer, LostAcknowledgement)
        Application.put_env(:query_service_ex, :run_test_owner, self())
        assert {:error, :append_uncertain} = record(input)
        assert_receive :append_attempted
        input
      end)

    with_core(context, fn ->
      assert {:ok, %{disposition: "already_recorded", execution: "none"}} = record(input)
      refute_receive :append_attempted
      assert {:ok, run} = Evidence.read(F.tenant(), F.uuid(1))
      assert run.revision == 4
      assert "unresolved_attempt" in run.unknowns

      next =
        command(F.payload("attempt.started", F.uuid(1), List.last(run.events)["id"]), 4)
        |> put_in(["event", "attempt_id"], F.uuid(201))

      assert {:error, :invalid_transition} = record(next)
      refute_receive :append_attempted
    end)
  end

  defp capture_history(count \\ 7) do
    F.history()
    |> Enum.take(count)
    |> Enum.reduce({[], []}, fn event, {commands, receipts} ->
      previous = if receipts == [], do: nil, else: List.last(receipts).event_id
      input = command(Map.put(event["payload"], "causation_id", previous), event["version"] - 1)
      assert {:ok, %{disposition: "recorded"} = receipt} = record(input)
      assert receipt.version == event["version"]
      assert receipt.execution == "none"
      {commands ++ [input], receipts ++ [receipt]}
    end)
  end

  defp command(payload, previous),
    do: %{
      "operation_id" => F.uuid(900 + previous),
      "expected_version" => previous,
      "event" => payload
    }

  defp record(input), do: Recorder.record(F.tenant(), F.uuid(1), input)
end
