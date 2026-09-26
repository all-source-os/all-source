defmodule QueryServiceEx.Application.Services.CustomerAgentReviewTest do
  use ExUnit.Case, async: true

  alias QueryServiceEx.Application.Services.CustomerAgentReview

  test "validates replay targets against the existing catalog" do
    input = %{
      "schema_version" => 1,
      "kind" => "replay_plan",
      "projection_name" => "event-count",
      "sources" => []
    }

    assert {:ok, _} = CustomerAgentReview.validate(input)

    assert {:error, :unknown_projection} =
             CustomerAgentReview.validate(Map.put(input, "projection_name", "invented"))
  end

  test "only discloses bounded counts, status and analysis timestamp" do
    analysis = %{
      projection_name: "event-count",
      projection_status: "ready",
      total_events: 10,
      sampled_events: 3,
      current_entity_count: 1,
      sampled_entity_count: 2,
      analysis_scope: "sample",
      analyzed_at: "2026-09-26T12:00:00Z",
      ready_to_replay: true,
      sampled_entities: [%{entity_id: "private-customer-id", event_count: 3}],
      event_type_distribution: [%{event_type: "private-event-name", count: 3}],
      checks: [%{detail: "private"}],
      warnings: ["private"],
      payload: "private"
    }

    assert {:ok, snapshot} = CustomerAgentReview.replay_snapshot(analysis)
    assert snapshot["reported_total_events"] == 10
    assert snapshot["sampled_events"] == 3
    assert snapshot["analysis_scope"] == "sample"

    assert snapshot["unknowns"] == [
             "total_count_provenance",
             "authoritative_order",
             "restart_proof",
             "run_comparison"
           ]

    refute Jason.encode!(snapshot) =~ "private"
    refute Map.has_key?(snapshot, "ready_to_replay")
    refute Map.has_key?(snapshot, "approved")
  end

  test "missing evidence is null, never an invented zero or proof" do
    assert {:ok, snapshot} =
             CustomerAgentReview.replay_snapshot(%{projection_name: "event-count"})

    assert is_nil(snapshot["reported_total_events"])
    assert is_nil(snapshot["sampled_events"])
    assert is_nil(snapshot["projection_status"])
    assert is_nil(snapshot["analyzed_at"])
  end

  test "rejects malformed or inconsistent internal snapshots instead of reflecting them" do
    for {key, value} <- [
          {:total_events, -1},
          {:sampled_events, 1_001},
          {:sampled_entity_count, 1_001},
          {:sampled_events, "3"},
          {:current_entity_count, 9_007_199_254_740_992},
          {:projection_status, "private-status"},
          {:analysis_scope, "anything"},
          {:analyzed_at, "yesterday"}
        ] do
      assert {:error, :invalid_analysis} =
               CustomerAgentReview.replay_snapshot(
                 Map.put(%{projection_name: "event-count"}, key, value)
               )
    end

    assert {:error, :invalid_analysis} =
             CustomerAgentReview.replay_snapshot(%{
               projection_name: "event-count",
               total_events: 2,
               sampled_events: 3
             })

    assert {:error, :invalid_analysis} =
             CustomerAgentReview.replay_snapshot(%{
               projection_name: "event-count",
               sampled_entity_count: 3,
               sampled_events: 2
             })

    assert {:error, :unknown_projection} =
             CustomerAgentReview.replay_snapshot(%{projection_name: "invented"})

    assert {:error, :invalid_analysis} = CustomerAgentReview.replay_snapshot(nil)
  end

  test "scope label cannot overstate evidence coverage" do
    for {scope, total, sampled} <- [{"full", 10, 3}, {"sample", 3, 3}] do
      assert {:error, :invalid_analysis} =
               CustomerAgentReview.replay_snapshot(%{
                 projection_name: "event-count",
                 analysis_scope: scope,
                 total_events: total,
                 sampled_events: sampled
               })
    end
  end
end
