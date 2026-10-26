defmodule QueryServiceEx.Application.UseCases.PipelineProcessorTest do
  use ExUnit.Case, async: true

  alias QueryServiceEx.Application.UseCases.PipelineProcessor
  alias QueryServiceEx.Domain.Entities.Pipeline
  alias QueryServiceEx.Domain.Entities.Pipeline.Definition

  setup do
    # Create a simple test pipeline
    pipeline =
      Definition.new(
        name: :test_pipeline,
        version: 1,
        operators: [
          Pipeline.filter_operator("active_only", fn e -> Map.get(e, :active, true) end),
          Pipeline.transform_operator("add_timestamp", fn e ->
            Map.put(e, :processed_at, "2024-01-01")
          end)
        ]
      )

    {:ok, pipeline: pipeline}
  end

  describe "start_link/1" do
    test "starts a pipeline processor with valid pipeline", %{pipeline: pipeline} do
      assert {:ok, pid} = PipelineProcessor.start_link(pipeline: pipeline)
      assert Process.alive?(pid)

      assert :ok = PipelineProcessor.stop(pid)
    end

    test "starts with a registered name", %{pipeline: pipeline} do
      assert {:ok, pid} =
               PipelineProcessor.start_link(pipeline: pipeline, name: :test_processor)

      assert Process.whereis(:test_processor) == pid
      assert Process.alive?(pid)

      assert :ok = PipelineProcessor.stop(pid)
    end

    test "accepts optional config", %{pipeline: pipeline} do
      assert {:ok, pid} =
               PipelineProcessor.start_link(
                 pipeline: pipeline,
                 config: %{batch_size: 100}
               )

      assert Process.alive?(pid)

      assert :ok = PipelineProcessor.stop(pid)
    end

    test "rejects invalid pipeline" do
      Process.flag(:trap_exit, true)

      invalid_pipeline = %Definition{
        name: "invalid_name",
        version: 1,
        operators: []
      }

      result = PipelineProcessor.start_link(pipeline: invalid_pipeline)

      case result do
        {:ok, pid} ->
          receive do
            {:EXIT, ^pid, {:invalid_pipeline, _}} ->
              assert true

            {:EXIT, ^pid, reason} ->
              assert match?({:invalid_pipeline, _}, reason)
          after
            100 -> flunk("Expected process to exit with invalid pipeline")
          end

        {:error, {:invalid_pipeline, _}} ->
          assert true
      end
    end
  end

  describe "process_event/2" do
    setup %{pipeline: pipeline} do
      pid = start_owned_processor(pipeline: pipeline)

      {:ok, pid: pid}
    end

    test "processes a valid event successfully", %{pid: pid} do
      event = %{id: 1, active: true, name: "test"}

      assert {:ok, processed} = PipelineProcessor.process_event(pid, event)
      assert processed.id == 1
      assert processed.processed_at == "2024-01-01"
    end

    test "filters event that doesn't pass pipeline", %{pid: pid} do
      event = %{id: 2, active: false, name: "filtered"}

      assert :filtered == PipelineProcessor.process_event(pid, event)
    end

    test "returns error when pipeline operator fails", %{pipeline: _pipeline} do
      error_pipeline =
        Definition.new(
          name: :error_pipeline,
          version: 1,
          operators: [
            Pipeline.transform_operator("fail", fn _e ->
              raise "Processing error"
            end)
          ]
        )

      pid = start_owned_processor(pipeline: error_pipeline)

      event = %{id: 1}
      assert {:error, "Processing error"} = PipelineProcessor.process_event(pid, event)
    end

    test "updates statistics after processing", %{pid: pid} do
      event = %{id: 1, active: true}
      PipelineProcessor.process_event(pid, event)

      stats = PipelineProcessor.get_stats(pid)
      assert stats.total_processed == 1
      assert stats.total_filtered == 0
      assert stats.total_errors == 0
    end

    test "updates statistics after filtering", %{pid: pid} do
      event = %{id: 1, active: false}
      PipelineProcessor.process_event(pid, event)

      stats = PipelineProcessor.get_stats(pid)
      assert stats.total_processed == 0
      assert stats.total_filtered == 1
      assert stats.total_errors == 0
    end
  end

  describe "process_batch/2" do
    setup %{pipeline: pipeline} do
      pid = start_owned_processor(pipeline: pipeline)

      {:ok, pid: pid}
    end

    test "processes multiple events successfully", %{pid: pid} do
      events = [
        %{id: 1, active: true},
        %{id: 2, active: true},
        %{id: 3, active: true}
      ]

      results = PipelineProcessor.process_batch(pid, events)

      assert length(results) == 3
      assert Enum.all?(results, fn e -> e.processed_at == "2024-01-01" end)
    end

    test "filters out inactive events in batch", %{pid: pid} do
      events = [
        %{id: 1, active: true},
        %{id: 2, active: false},
        %{id: 3, active: true},
        %{id: 4, active: false}
      ]

      results = PipelineProcessor.process_batch(pid, events)

      assert length(results) == 2
      assert Enum.map(results, & &1.id) == [1, 3]
    end

    test "handles empty batch", %{pid: pid} do
      results = PipelineProcessor.process_batch(pid, [])
      assert results == []
    end

    test "updates statistics for batch processing", %{pid: pid} do
      events = [
        %{id: 1, active: true},
        %{id: 2, active: false},
        %{id: 3, active: true}
      ]

      PipelineProcessor.process_batch(pid, events)

      stats = PipelineProcessor.get_stats(pid)
      assert stats.total_processed == 2
      assert stats.total_filtered == 1
    end

    test "handles large batches efficiently", %{pid: pid} do
      events = Enum.map(1..1000, fn i -> %{id: i, active: rem(i, 2) == 0} end)

      results = PipelineProcessor.process_batch(pid, events)

      assert length(results) == 500
      assert Enum.all?(results, fn e -> rem(e.id, 2) == 0 end)
    end
  end

  describe "get_stats/1" do
    setup %{pipeline: pipeline} do
      pid = start_owned_processor(pipeline: pipeline)

      {:ok, pid: pid}
    end

    test "returns initial statistics", %{pid: pid} do
      stats = PipelineProcessor.get_stats(pid)

      assert stats.total_processed == 0
      assert stats.total_filtered == 0
      assert stats.total_errors == 0
      assert stats.success_rate == 0.0
    end

    test "calculates success rate correctly", %{pid: pid} do
      events = [
        %{id: 1, active: true},
        %{id: 2, active: false},
        %{id: 3, active: true},
        %{id: 4, active: true}
      ]

      PipelineProcessor.process_batch(pid, events)

      stats = PipelineProcessor.get_stats(pid)
      assert stats.total_processed == 3
      assert stats.total_filtered == 1
      assert stats.success_rate == 75.0
    end

    test "handles error rate in statistics", %{pipeline: _pipeline} do
      error_pipeline =
        Definition.new(
          name: :error_pipeline,
          version: 1,
          operators: [
            Pipeline.transform_operator("conditional_fail", fn e ->
              if rem(e.id, 2) == 0, do: e, else: raise("Odd number error")
            end)
          ]
        )

      pid = start_owned_processor(pipeline: error_pipeline)

      events = [%{id: 1}, %{id: 2}, %{id: 3}, %{id: 4}]
      PipelineProcessor.process_batch(pid, events)

      stats = PipelineProcessor.get_stats(pid)
      assert stats.total_processed == 2
      assert stats.total_errors == 2
      assert stats.success_rate == 50.0
    end
  end

  describe "reset_stats/1" do
    setup %{pipeline: pipeline} do
      pid = start_owned_processor(pipeline: pipeline)

      {:ok, pid: pid}
    end

    test "resets all statistics to zero", %{pid: pid} do
      # Process some events
      events = [
        %{id: 1, active: true},
        %{id: 2, active: false}
      ]

      PipelineProcessor.process_batch(pid, events)

      # Verify stats were updated
      stats_before = PipelineProcessor.get_stats(pid)
      assert stats_before.total_processed > 0

      # Reset stats
      assert :ok = PipelineProcessor.reset_stats(pid)

      # Verify stats are zero
      stats_after = PipelineProcessor.get_stats(pid)
      assert stats_after.total_processed == 0
      assert stats_after.total_filtered == 0
      assert stats_after.total_errors == 0
      assert stats_after.success_rate == 0.0
    end
  end

  describe "get_pipeline/1" do
    setup %{pipeline: pipeline} do
      pid = start_owned_processor(pipeline: pipeline)

      {:ok, pid: pid}
    end

    test "returns the pipeline definition", %{pid: pid, pipeline: pipeline} do
      returned_pipeline = PipelineProcessor.get_pipeline(pid)

      assert returned_pipeline.name == pipeline.name
      assert returned_pipeline.version == pipeline.version
      assert length(returned_pipeline.operators) == length(pipeline.operators)
    end
  end

  describe "concurrent processing" do
    setup %{pipeline: pipeline} do
      pid = start_owned_processor(pipeline: pipeline)

      {:ok, pid: pid}
    end

    test "handles concurrent event processing", %{pid: pid} do
      # Spawn multiple tasks to process events concurrently
      tasks =
        Enum.map(1..10, fn i ->
          Task.async(fn ->
            PipelineProcessor.process_event(pid, %{id: i, active: true})
          end)
        end)

      results = Task.await_many(tasks)

      assert length(results) == 10
      assert Enum.all?(results, fn {:ok, _} -> true end)

      stats = PipelineProcessor.get_stats(pid)
      assert stats.total_processed == 10
    end
  end

  describe "complex pipeline scenarios" do
    test "multi-stage data enrichment pipeline" do
      pipeline =
        Definition.new(
          name: :enrichment_pipeline,
          version: 1,
          operators: [
            Pipeline.filter_operator("valid_orders", fn e ->
              Map.has_key?(e, :order_id) and e.order_id != nil
            end),
            Pipeline.transform_operator("normalize", fn e ->
              Map.put(e, :normalized, true)
            end),
            Pipeline.enrich_operator("add_metadata", fn e ->
              Map.put(e, :metadata, %{processed_by: "pipeline", timestamp: "2024-01-01"})
            end),
            Pipeline.validate_operator("validate_complete", fn e ->
              if Map.has_key?(e, :metadata), do: e, else: raise("Missing metadata")
            end)
          ]
        )

      pid = start_owned_processor(pipeline: pipeline)

      events = [
        %{order_id: "ORD-1", amount: 100},
        %{order_id: nil, amount: 50},
        %{order_id: "ORD-2", amount: 200},
        %{amount: 75}
      ]

      results = PipelineProcessor.process_batch(pid, events)

      assert length(results) == 2
      assert Enum.all?(results, fn e -> e.normalized == true end)
      assert Enum.all?(results, fn e -> Map.has_key?(e, :metadata) end)
    end

    test "event routing pipeline" do
      pipeline =
        Definition.new(
          name: :routing_pipeline,
          version: 1,
          operators: [
            Pipeline.transform_operator("classify", fn e ->
              priority = if e.amount > 1000, do: :high, else: :normal
              Map.put(e, :priority, priority)
            end),
            Pipeline.route_operator("route_by_priority", fn e ->
              case e.priority do
                :high -> :priority_queue
                :normal -> :standard_queue
              end
            end)
          ]
        )

      pid = start_owned_processor(pipeline: pipeline)

      events = [
        %{id: 1, amount: 500},
        %{id: 2, amount: 1500},
        %{id: 3, amount: 2000}
      ]

      results = PipelineProcessor.process_batch(pid, events)

      assert length(results) == 3

      high_priority = Enum.filter(results, fn e -> e.priority == :high end)
      normal_priority = Enum.filter(results, fn e -> e.priority == :normal end)

      assert length(high_priority) == 2
      assert length(normal_priority) == 1
    end
  end

  describe "telemetry integration" do
    test "emits telemetry events on start" do
      test_pid = self()

      :telemetry.attach(
        "test-processor-start",
        [:pipeline_processor, :started],
        fn _event, measurements, metadata, _config ->
          send(test_pid, {:telemetry, measurements, metadata})
        end,
        nil
      )

      pipeline =
        Definition.new(
          name: :telemetry_test,
          version: 1,
          operators: []
        )

      start_owned_processor(pipeline: pipeline)

      assert_receive {:telemetry, %{count: 1}, %{pipeline: :telemetry_test, version: 1}}, 100

      on_exit(fn ->
        :telemetry.detach("test-processor-start")
      end)
    end

    test "emits telemetry events on event processing" do
      test_pid = self()

      :telemetry.attach(
        "test-event-processed",
        [:pipeline_processor, :event_processed],
        fn _event, measurements, metadata, _config ->
          send(test_pid, {:telemetry, measurements, metadata})
        end,
        nil
      )

      pipeline =
        Definition.new(
          name: :telemetry_test,
          version: 1,
          operators: [
            Pipeline.transform_operator("test", fn e -> e end)
          ]
        )

      pid = start_owned_processor(pipeline: pipeline)
      PipelineProcessor.process_event(pid, %{id: 1})

      assert_receive {:telemetry, measurements, %{pipeline: :telemetry_test}}, 100
      assert Map.has_key?(measurements, :duration)
      assert Map.has_key?(measurements, :count)

      on_exit(fn ->
        :telemetry.detach("test-event-processed")
      end)
    end
  end

  defp start_owned_processor(options) do
    spec =
      Supervisor.child_spec({PipelineProcessor, options}, id: make_ref(), restart: :temporary)

    start_supervised!(spec)
  end
end
