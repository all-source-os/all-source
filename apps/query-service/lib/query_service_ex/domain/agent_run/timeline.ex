defmodule QueryServiceEx.Domain.AgentRun.Timeline do
  @moduledoc """
  Read-only fold of a complete retained run. Versions and causation establish
  order; timestamps never repair missing or conflicting sequence evidence.
  """
  alias QueryServiceEx.Domain.AgentRun.Event
  @limit 1_000

  def limit, do: @limit

  @doc "Check a next payload against an already validated timeline, without recording it."
  def validate_next(nil, %{"kind" => "run.started", "causation_id" => nil} = payload) do
    case Event.validate(payload) do
      {:ok, _} -> :ok
      _ -> {:error, :invalid_run_event}
    end
  end

  def validate_next(nil, _), do: {:error, :invalid_transition}

  def validate_next(run, payload) do
    with {:ok, _} <- Event.validate(payload),
         true <- payload["run_id"] == run.run_id,
         true <- payload["causation_id"] == List.last(run.events)["id"] do
      state = %{
        changes: Map.new(run.changes, &{&1.id, &1}),
        attempts: Map.new(run.attempts, &{&1.id, &1}),
        completed: run.completed,
        capture_gap: "capture_gap" in run.unknowns
      }

      case step(Map.put(payload, "version", run.revision + 1), {:ok, state}) do
        {:cont, {:ok, _}} -> :ok
        {:halt, error} -> error
      end
    else
      false -> {:error, :stale_revision}
      error -> error
    end
  end

  def build(tenant, run_id, events) when is_list(events) and length(events) <= @limit do
    with true <- is_binary(tenant) and Event.uuid?(run_id),
         {:ok, records} <- decode(events, tenant, run_id) do
      ordered = Enum.sort_by(records, & &1["version"])

      cond do
        ordered == [] -> {:error, :not_found}
        not ordered?(ordered) -> {:error, :order_uncertain}
        true -> fold(run_id, ordered)
      end
    else
      _ -> {:error, :invalid_run_evidence}
    end
  end

  def build(_, _, _), do: {:error, :run_too_large}

  defp decode(events, tenant, run_id) do
    Enum.reduce_while(events, {:ok, []}, fn event, {:ok, records} ->
      case Event.stored(event, tenant, run_id) do
        {:ok, record} -> {:cont, {:ok, [record | records]}}
        error -> {:halt, error}
      end
    end)
  end

  defp ordered?(records) do
    versions = Enum.map(records, & &1["version"])
    ids = Enum.map(records, & &1["id"])
    causes = Enum.map(records, & &1["causation_id"])

    versions == Enum.to_list(1..length(records)) and
      length(Enum.uniq(ids)) == length(ids) and causes == [nil | Enum.drop(ids, -1)] and
      hd(records)["kind"] == "run.started"
  end

  defp fold(run_id, records) do
    initial = %{changes: %{}, attempts: %{}, completed: false, capture_gap: false}

    result = Enum.reduce_while(Enum.drop(records, 1), {:ok, initial}, &step/2)

    with {:ok, state} <- result do
      changes = state.changes |> Map.values() |> Enum.sort_by(& &1.number)
      attempts = state.attempts |> Map.values() |> Enum.sort_by(& &1.started_version)
      unresolved = Enum.any?(attempts, &(&1.state in ~w(started tested unknown)))
      unknowns = if state.capture_gap, do: ["capture_gap"], else: []
      unknowns = if unresolved, do: unknowns ++ ["unresolved_attempt"], else: unknowns
      unknowns = if state.completed, do: unknowns, else: unknowns ++ ["run_open"]
      first = hd(records)

      {:ok,
       %{
         run_id: run_id,
         revision: List.last(records)["version"],
         digest: digest(records),
         events: records,
         changes: changes,
         attempts: attempts,
         unknowns: unknowns,
         completed: state.completed,
         order: "server_version",
         restart_proof: "not_established",
         approval_authority: "not_established",
         agent_sha256: first["agent_sha256"],
         model_sha256: first["model_sha256"],
         prompt_sha256: first["prompt_sha256"]
       }}
    end
  end

  defp step(_event, {:ok, %{completed: true}}), do: {:halt, {:error, :invalid_transition}}

  defp step(event, {:ok, state}) do
    case apply_event(event["kind"], event, state) do
      {:ok, next} -> {:cont, {:ok, next}}
      _ -> {:halt, {:error, :invalid_transition}}
    end
  end

  defp apply_event("capture_gap", _, state), do: {:ok, %{state | capture_gap: true}}
  defp apply_event("run.completed", _, state), do: {:ok, %{state | completed: true}}

  defp apply_event("change.proposed", event, state) do
    id = event["change_id"]

    if not Map.has_key?(state.changes, id) and
         event["change_number"] == map_size(state.changes) + 1 do
      change = %{
        id: id,
        number: event["change_number"],
        state: "proposed",
        evidence_sha256: event["evidence_sha256"]
      }

      {:ok, %{state | changes: Map.put(state.changes, id, change)}}
    else
      {:error, :invalid_transition}
    end
  end

  defp apply_event(kind, event, state) when kind in ~w(change.approved change.abandoned) do
    with {:ok, change} <- change(event, state),
         true <- change_transition?(kind, change.state),
         false <- Enum.any?(state.attempts, fn {_, attempt} -> attempt.change_id == change.id end) do
      status = if kind == "change.approved", do: "approved_recorded", else: "abandoned"
      {:ok, put_change(state, %{change | state: status})}
    end
  end

  defp apply_event("attempt.started", event, state) do
    with {:ok, %{state: "approved_recorded"} = change} <- change(event, state),
         false <- Map.has_key?(state.attempts, event["attempt_id"]),
         false <- unresolved?(state, change.id) do
      attempt = %{
        id: event["attempt_id"],
        change_id: change.id,
        started_version: event["version"],
        state: "started",
        test_outcome: nil,
        test_sha256: nil
      }

      {:ok, put_attempt(state, attempt)}
    end
  end

  defp apply_event("attempt.tested", event, state) do
    with {:ok, %{state: "started"} = attempt} <- attempt(event, state) do
      {:ok,
       put_attempt(state, %{
         attempt
         | state: "tested",
           test_outcome: event["outcome"],
           test_sha256: event["evidence_sha256"]
       })}
    end
  end

  defp apply_event(kind, event, state) do
    with {:ok, attempt} <- attempt(event, state),
         true <- latest_attempt?(state, attempt),
         {:ok, status} <- outcome(kind, event, attempt) do
      next = put_attempt(state, %{attempt | state: status})

      case status do
        "accepted" ->
          {:ok, put_change(next, %{next.changes[attempt.change_id] | state: "accepted"})}

        "reverted" ->
          {:ok, put_change(next, %{next.changes[attempt.change_id] | state: "approved_recorded"})}

        _ ->
          {:ok, next}
      end
    end
  end

  defp change_transition?("change.approved", "proposed"), do: true

  defp change_transition?("change.abandoned", state) when state in ~w(proposed approved_recorded),
    do: true

  defp change_transition?(_, _), do: false

  defp outcome("attempt.accepted", _, %{state: "tested", test_outcome: "pass"}),
    do: {:ok, "accepted"}

  defp outcome("attempt.failed", _, %{state: status}) when status in ~w(started tested),
    do: {:ok, "failed"}

  defp outcome("attempt.unknown", _, %{state: status}) when status in ~w(started tested),
    do: {:ok, "unknown"}

  defp outcome("attempt.reverted", _, %{state: status}) when status in ~w(tested failed accepted),
    do: {:ok, "reverted"}

  defp outcome("attempt.reconciled", %{"outcome" => "failed"}, %{state: "unknown"}),
    do: {:ok, "failed"}

  defp outcome("attempt.reconciled", %{"outcome" => "succeeded"}, %{state: "unknown"}),
    do: {:ok, "started"}

  defp outcome(_, _, _), do: {:error, :invalid_transition}

  defp change(event, state) do
    expected = event["change_number"]

    case state.changes[event["change_id"]] do
      %{number: ^expected} = change -> {:ok, change}
      _ -> {:error, :invalid_transition}
    end
  end

  defp attempt(event, state) do
    with {:ok, change} <- change(event, state),
         %{change_id: id} = attempt <- state.attempts[event["attempt_id"]],
         true <- id == change.id do
      {:ok, attempt}
    else
      _ -> {:error, :invalid_transition}
    end
  end

  defp unresolved?(state, id),
    do:
      Enum.any?(state.attempts, fn {_, a} ->
        a.change_id == id and a.state in ~w(started tested unknown)
      end)

  defp latest_attempt?(state, attempt),
    do:
      not Enum.any?(state.attempts, fn {_, other} ->
        other.change_id == attempt.change_id and other.started_version > attempt.started_version
      end)

  defp put_change(state, change),
    do: %{state | changes: Map.put(state.changes, change.id, change)}

  defp put_attempt(state, attempt),
    do: %{state | attempts: Map.put(state.attempts, attempt.id, attempt)}

  defp digest(records) do
    canonical =
      Enum.map(records, fn record ->
        Enum.map(Event.keys() ++ ~w(id version timestamp), &record[&1])
      end)

    :sha256
    |> :crypto.hash(Jason.encode!(["agent-run-evidence-v1", canonical]))
    |> Base.encode16(case: :lower)
  end
end
