defmodule QueryServiceEx.TestSupport.AgentRunFixture do
  @moduledoc false
  alias QueryServiceEx.Domain.AgentRun.Event

  def uuid(number),
    do: "00000000-0000-4000-8000-" <> String.pad_leading(to_string(number), 12, "0")

  def hash(number), do: :sha256 |> :crypto.hash(to_string(number)) |> Base.encode16(case: :lower)
  def tenant, do: "synthetic-run-evidence"

  def payload(kind, run_id, previous, extra \\ %{}) do
    defaults =
      Event.keys()
      |> Map.new(&{&1, nil})
      |> Map.merge(%{
        "schema_version" => 1,
        "run_id" => run_id,
        "kind" => kind,
        "causation_id" => previous
      })

    values =
      case kind do
        "run.started" ->
          %{"agent_sha256" => hash(1), "model_sha256" => hash(2), "prompt_sha256" => hash(3)}

        kind when kind in ~w(run.completed capture_gap) ->
          %{}

        kind ->
          %{"change_id" => uuid(100), "change_number" => 1, "evidence_sha256" => hash(4)}
          |> Map.put(
            "attempt_id",
            if(String.starts_with?(kind, "attempt."), do: uuid(200), else: nil)
          )
      end

    defaults |> Map.merge(values) |> Map.merge(extra)
  end

  def history(run_id \\ uuid(1), outcome \\ "pass") do
    last = if outcome == "pass", do: "attempt.accepted", else: "attempt.reverted"

    kinds =
      ~w(run.started change.proposed change.approved attempt.started attempt.tested) ++
        [last, "run.completed"]

    kinds
    |> Enum.with_index(1)
    |> Enum.map(fn {kind, version} ->
      previous = if version == 1, do: nil, else: uuid(1000 + version - 1)
      extra = if kind == "attempt.tested", do: %{"outcome" => outcome}, else: %{}
      stored(payload(kind, run_id, previous, extra), version)
    end)
  end

  def stored(payload, version, tenant \\ tenant()) do
    %{
      "id" => uuid(1000 + version),
      "version" => version,
      "timestamp" => "2026-09-27T09:00:00Z",
      "tenant_id" => tenant,
      "entity_id" => Event.entity(tenant, payload["run_id"]),
      "event_type" => "agent_run.v1." <> payload["kind"],
      "payload" => payload,
      "metadata" => %{"private" => "synthetic do not disclose"}
    }
  end
end
