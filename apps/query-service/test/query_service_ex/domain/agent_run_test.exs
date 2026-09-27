defmodule QueryServiceEx.Domain.AgentRunTest do
  use ExUnit.Case, async: true
  alias QueryServiceEx.Application.Services.AgentRunEvidence
  alias QueryServiceEx.Domain.AgentRun.{Comparison, Event, Timeline}
  alias QueryServiceEx.TestSupport.AgentRunFixture, as: F

  test "all valid events are allowlisted and unknown or sensitive fields fail closed" do
    for record <- F.history() do
      assert {:ok, _} = Event.validate(record["payload"])

      assert {:error, :invalid_run_event} =
               Event.validate(Map.put(record["payload"], "prompt", "private"))

      assert {:error, :invalid_run_event} =
               Event.validate(Map.put(record["payload"], "approved", true))
    end

    for value <- [nil, [], "private", %{}, %{"schema_version" => 1}] do
      assert {:error, :invalid_run_event} = Event.validate(value)
    end
  end

  test "server versions order records and private envelope fields stay out" do
    assert {:ok, run} = build(Enum.reverse(F.history()))
    assert run.revision == 7
    assert Enum.map(run.events, & &1["version"]) == Enum.to_list(1..7)
    assert run.unknowns == []
    assert [%{state: "accepted", number: 1}] = run.changes
    assert run.approval_authority == "not_established"
    assert run.restart_proof == "not_established"
    refute Jason.encode!(run) =~ "private"
    assert {:ok, same} = build(F.history())
    assert same.digest == run.digest
  end

  test "missing, duplicated and legacy constant versions cannot invent order" do
    records = F.history()

    for bad <- [
          tl(records),
          records ++ [hd(records)],
          Enum.map(records, &Map.put(&1, "version", 1))
        ] do
      assert {:error, :order_uncertain} = build(bad)
    end

    bad_cause =
      update_in(records, [Access.at(3), "payload", "causation_id"], fn _ -> F.uuid(999) end)

    assert {:error, :order_uncertain} = build(bad_cause)
    duplicate_id = put_in(records, [Access.at(3), "id"], hd(records)["id"])
    assert {:error, :order_uncertain} = build(duplicate_id)
  end

  test "wrong tenant, entity, event namespace or run cannot disclose evidence" do
    record = hd(F.history())

    for field <- ~w(tenant_id entity_id event_type) do
      assert {:error, :invalid_run_evidence} = build([Map.put(record, field, "other")])
    end

    assert {:error, :invalid_run_evidence} = Timeline.build("other", F.uuid(1), F.history())
    assert {:error, :invalid_run_evidence} = Timeline.build(F.tenant(), F.uuid(2), F.history())
  end

  test "empty and over-limit history have explicit incomplete states" do
    assert {:error, :not_found} = build([])
    assert {:error, :run_too_large} = build(List.duplicate(hd(F.history()), 1001))
  end

  test "unfinished action is unknown and cannot authorize retry" do
    assert {:ok, run} = build(Enum.take(F.history(), 4))
    assert "unresolved_attempt" in run.unknowns
    assert "run_open" in run.unknowns
    assert Comparison.retry_decision(run, 1) == :unknown
    assert Comparison.retry_decision(run, 999) == :unknown
  end

  test "one recorded failure permits advisory retry; two stop and never execute" do
    failed = F.history(F.uuid(1), "fail") |> Enum.drop(-1)
    assert {:ok, one} = build(failed)
    assert Comparison.retry_decision(one, 1) == :allow

    second =
      ["attempt.started", "attempt.tested", "attempt.reverted"]
      |> Enum.with_index(7)
      |> Enum.map(fn {kind, version} ->
        extra = %{
          "attempt_id" => F.uuid(201),
          "outcome" => if(kind == "attempt.tested", do: "fail", else: nil)
        }

        F.payload(kind, F.uuid(1), F.uuid(1000 + version - 1), extra) |> F.stored(version)
      end)

    assert {:ok, two} = build(failed ++ second)
    assert Comparison.retry_decision(two, 1) == :stop
    assert Comparison.retry_decision(two, 1, 3) == :allow
    assert Comparison.retry_decision(two, 1, 0) == :unknown
  end

  test "test failure cannot be accepted and repeated approval does not change authority" do
    records = F.history(F.uuid(1), "fail")

    invalid =
      update_in(records, [Access.at(5)], fn event ->
        event
        |> Map.put("event_type", "agent_run.v1.attempt.accepted")
        |> put_in(["payload", "kind"], "attempt.accepted")
      end)

    assert {:error, :invalid_transition} = build(invalid)
    assert {:ok, accepted} = build(F.history())
    assert Comparison.retry_decision(accepted, 1) == :stop
  end

  test "comparison aligns numbered changes, pins both digests and never calls an executor" do
    assert {:ok, left} = build(F.history())
    assert {:ok, right} = Timeline.build(F.tenant(), F.uuid(2), F.history(F.uuid(2), "fail"))
    report = Comparison.compare(left, right)
    assert report.state == "divergent"
    assert report.first_divergence.change_number == 1
    assert report.baseline.digest == left.digest
    assert report.candidate.digest == right.digest
    assert report.execution == "none"
    assert Comparison.compare(left, left).state == "same_recorded_evidence"
    assert {:ok, unknown} = build(Enum.take(F.history(), 4))
    assert Comparison.compare(unknown, unknown).state == "inconclusive"
  end

  test "version pagination is pinned to exact history and refuses changed revisions" do
    assert {:ok, run} = build(F.history())
    assert {:ok, first} = AgentRunEvidence.page(run, run.digest, 0, 3)
    assert first.next_version == 3
    assert {:ok, last} = AgentRunEvidence.page(run, run.digest, first.next_version, 100)
    assert last.next_version == nil
    assert length(first.events ++ last.events) == 7
    assert {:error, :stale_revision} = AgentRunEvidence.page(run, F.hash(9), 3, 100)
    assert {:error, :invalid_page} = AgentRunEvidence.page(run, run.digest, -1, 100)
  end

  test "changed recorded approval evidence is visible even with identical final outcome" do
    records = F.history()
    assert {:ok, baseline} = build(records)
    changed = put_in(records, [Access.at(2), "payload", "evidence_sha256"], F.hash(999))
    assert {:ok, candidate} = build(changed)
    report = Comparison.compare(baseline, candidate)
    assert report.state == "divergent"
    assert report.first_divergence.change_number == 1
    refute report.baseline.digest == report.candidate.digest
  end

  test "capture gaps preserve evidence but prevent retry and conclusive equality" do
    records = F.history() |> Enum.take(3) |> append("capture_gap")
    assert {:ok, run} = build(records)
    assert "capture_gap" in run.unknowns
    assert Comparison.retry_decision(run, 1) == :unknown
    assert Comparison.compare(run, run).state == "inconclusive"
  end

  test "abandoned before action remains recorded and cannot be retried" do
    for count <- [2, 3] do
      assert {:ok, run} = build(Enum.take(F.history(), count) |> append("change.abandoned"))
      assert [%{state: "abandoned"}] = run.changes
      assert run.attempts == []
      assert Comparison.retry_decision(run, 1) == :stop
    end
  end

  test "unknown external outcome requires evidence-backed reconciliation before retry" do
    records = Enum.take(F.history(), 4) |> append("attempt.unknown")
    assert {:ok, unknown} = build(records)
    assert Comparison.retry_decision(unknown, 1) == :unknown

    assert {:ok, reconciled} =
             build(append(records, "attempt.reconciled", %{"outcome" => "failed"}))

    assert Comparison.retry_decision(reconciled, 1) == :allow

    assert {:error, :invalid_run_evidence} =
             build(
               append(records, "attempt.reconciled", %{
                 "outcome" => "failed",
                 "evidence_sha256" => nil
               })
             )

    assert {:ok, completed} =
             build(
               append(
                 append(records, "attempt.reconciled", %{"outcome" => "failed"}),
                 "run.completed"
               )
             )

    assert Comparison.retry_decision(completed, 1) == :stop
  end

  test "type confusion and changing descriptors mid-run are rejected" do
    first = hd(F.history())["payload"]
    assert {:error, :invalid_run_event} = Event.validate(Map.put(first, "schema_version", 1.0))
    later = Enum.at(F.history(), 1)["payload"]

    assert {:error, :invalid_run_event} =
             Event.validate(Map.put(later, "model_sha256", F.hash(99)))

    assert {:error, :invalid_run_event} = Event.validate(Map.put(later, "change_number", 1.0))
  end

  test "late reversion of an old failed attempt cannot reopen a later accepted change" do
    records =
      Enum.take(F.history(), 4)
      |> append("attempt.failed")
      |> append("attempt.started", %{"attempt_id" => F.uuid(201)})
      |> append("attempt.tested", %{"attempt_id" => F.uuid(201), "outcome" => "pass"})
      |> append("attempt.accepted", %{"attempt_id" => F.uuid(201)})

    assert {:ok, accepted} = build(records)
    assert Comparison.retry_decision(accepted, 1) == :stop
    assert {:error, :invalid_transition} = build(append(records, "attempt.reverted"))
  end

  defp append(records, kind, extra \\ %{}) do
    version = length(records) + 1
    payload = F.payload(kind, F.uuid(1), List.last(records)["id"], extra)
    records ++ [F.stored(payload, version)]
  end

  defp build(events), do: Timeline.build(F.tenant(), F.uuid(1), events)
end
