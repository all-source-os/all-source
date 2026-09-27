defmodule QueryServiceEx.Application.Services.AgentRunRecorderTest do
  use ExUnit.Case, async: false
  alias QueryServiceEx.Application.Services.AgentRunRecorder
  alias QueryServiceEx.TestSupport.AgentRunFixture, as: F

  defmodule Store do
    use Agent
    def start_link(opts), do: Agent.start_link(fn -> opts end, name: __MODULE__)

    def events(_, _) do
      Agent.get_and_update(__MODULE__, fn opts ->
        send(opts[:owner], :read)
        [result | remaining] = opts[:reads]
        {result, Keyword.put(opts, :reads, remaining)}
      end)
    end

    def append(request) do
      Agent.get(__MODULE__, fn opts ->
        send(opts[:owner], {:write, request})
        opts[:append]
      end)
    end
  end

  setup do
    keys = [:agent_run_source, :agent_run_writer]
    previous = Enum.map(keys, &{&1, Application.fetch_env!(:query_service_ex, &1)})
    Enum.each(keys, &Application.put_env(:query_service_ex, &1, Store))

    on_exit(fn ->
      Enum.each(previous, fn {key, value} ->
        Application.put_env(:query_service_ex, key, value)
      end)
    end)

    :ok
  end

  test "invalid command and unavailable source cannot reach the writer" do
    fixture([{:error, :source_unavailable}], nil)
    assert {:error, :invalid_append_command} = record(%{})
    refute_receive :read
    assert {:error, :source_unavailable} = record(input())
    assert_receive :read
    refute_receive {:write, _}
  end

  test "an uncertain write is never retried automatically" do
    fixture([{:ok, []}], {:error, :append_uncertain})
    assert {:error, :append_uncertain} = record(input())
    assert_receive :read
    assert_receive {:write, _}
    refute_receive {:write, _}
    refute_receive :read
  end

  test "success-shaped transport cannot substitute for matching stored evidence" do
    ack = %{"event_id" => F.uuid(1001), "version" => 1, "timestamp" => "2026-09-27T09:00:00Z"}
    fixture([{:ok, []}, {:ok, []}], {:ok, ack})
    assert {:error, :append_uncertain} = record(input())
    assert_receive {:write, _}
    refute_receive {:write, _}
  end

  test "conflict with still-unavailable evidence stays unknown" do
    fixture([{:ok, []}, {:error, :source_unavailable}], {:error, :version_conflict})
    assert {:error, :append_uncertain} = record(input())
    assert_receive {:write, _}
    refute_receive {:write, _}
  end

  defp fixture(reads, append),
    do: start_supervised!({Store, owner: self(), reads: reads, append: append})

  defp input,
    do: %{
      "operation_id" => F.uuid(900),
      "expected_version" => 0,
      "event" => F.payload("run.started", F.uuid(1), nil)
    }

  defp record(input), do: AgentRunRecorder.record(F.tenant(), F.uuid(1), input)
end
