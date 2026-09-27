defmodule QueryServiceEx.Domain.AgentRun.Comparison do
  @moduledoc "Compares validated recorded runs. Never executes a tool or projection rebuild."

  def compare(baseline, candidate) do
    left = Map.new(baseline.changes, &{&1.number, signature(&1, baseline)})
    right = Map.new(candidate.changes, &{&1.number, signature(&1, candidate)})
    numbers = (Map.keys(left) ++ Map.keys(right)) |> Enum.uniq() |> Enum.sort()

    differences =
      for number <- numbers, left[number] != right[number] do
        %{change_number: number, baseline: left[number], candidate: right[number]}
      end

    descriptor_changes =
      Enum.filter(
        [:agent_sha256, :model_sha256, :prompt_sha256],
        &(baseline[&1] != candidate[&1])
      )

    unknowns = Enum.uniq(baseline.unknowns ++ candidate.unknowns)

    %{
      state: state(differences, descriptor_changes, unknowns),
      baseline: Map.take(baseline, [:run_id, :revision, :digest]),
      candidate: Map.take(candidate, [:run_id, :revision, :digest]),
      descriptor_changes: descriptor_changes,
      first_divergence: List.first(differences),
      differences: differences,
      unknowns: unknowns,
      execution: "none",
      approval_authority: "not_established"
    }
  end

  @doc "Advisory decision for a future SDK wrapper; recorded approval is not authority."
  def retry_decision(run, change_number, failed_cap \\ 2)

  def retry_decision(run, number, cap) when is_integer(cap) and cap in 1..100 do
    case Enum.find(run.changes, &(&1.number == number)) do
      nil -> :unknown
      %{state: state} when state in ~w(accepted abandoned) -> :stop
      %{state: "proposed"} -> :unknown
      change -> attempts_decision(run, change.id, cap)
    end
  end

  def retry_decision(_, _, _), do: :unknown

  defp attempts_decision(run, id, cap) do
    attempts = Enum.filter(run.attempts, &(&1.change_id == id))

    cond do
      "capture_gap" in run.unknowns -> :unknown
      Enum.any?(attempts, &(&1.state in ~w(started tested unknown))) -> :unknown
      run.completed -> :stop
      Enum.count(attempts, &failed?/1) >= cap -> :stop
      true -> :allow
    end
  end

  defp failed?(%{state: "failed"}), do: true
  defp failed?(%{state: "reverted", test_outcome: "fail"}), do: true
  defp failed?(_), do: false

  defp signature(change, run) do
    history =
      run.attempts
      |> Enum.filter(&(&1.change_id == change.id))
      |> Enum.map(&Map.take(&1, [:state, :test_outcome, :test_sha256]))

    evidence =
      run.events
      |> Enum.filter(&(&1["change_id"] == change.id))
      |> Enum.map(&Map.take(&1, ~w(kind evidence_sha256 outcome)))

    %{
      state: change.state,
      evidence_sha256: change.evidence_sha256,
      attempts: history,
      evidence: evidence
    }
  end

  defp state([], [], []), do: "same_recorded_evidence"
  defp state([], [], _), do: "inconclusive"
  defp state(_, _, _), do: "divergent"
end
