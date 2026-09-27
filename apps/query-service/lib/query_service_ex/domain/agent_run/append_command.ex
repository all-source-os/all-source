defmodule QueryServiceEx.Domain.AgentRun.AppendCommand do
  @moduledoc """
  Typed conditional append. Operation hashes identify retries only within a
  complete retained run; they are neither credentials nor execution receipts.
  """
  alias QueryServiceEx.Domain.AgentRun.Event
  alias QueryServiceEx.Domain.AgentRun.Timeline
  @fields ~w(event expected_version operation_id)
  @metadata_key "agent_run_command_sha256"

  @enforce_keys [:tenant, :run_id, :expected_version, :payload, :command_sha256]
  defstruct [:tenant, :run_id, :expected_version, :payload, :command_sha256]

  def new(tenant, run_id, input) when is_map(input) do
    with true <- Event.tenant?(tenant) and Event.uuid?(run_id),
         true <- Enum.sort(Map.keys(input)) == @fields,
         true <- Event.uuid?(input["operation_id"]),
         version when is_integer(version) and version in 0..999 <- input["expected_version"],
         {:ok, payload} <- Event.validate(input["event"]),
         true <- payload["run_id"] == run_id do
      hash =
        :sha256
        |> :crypto.hash(
          Jason.encode!(["agent-run-command-v1", tenant, run_id, input["operation_id"]])
        )
        |> Base.encode16(case: :lower)

      {:ok,
       %__MODULE__{
         tenant: tenant,
         run_id: run_id,
         expected_version: version,
         payload: payload,
         command_sha256: hash
       }}
    else
      _ -> {:error, :invalid_append_command}
    end
  end

  def new(_, _, _), do: {:error, :invalid_append_command}

  def prepare(%__MODULE__{} = command, events) do
    with {:ok, run} <- history(command, events) do
      matches = Enum.filter(events, &(metadata_hash(&1) == command.command_sha256))

      case matches do
        [] -> fresh(command, run)
        [event] -> recover(command, event)
        _ -> {:error, :operation_conflict}
      end
    end
  end

  defp history(_, []), do: {:ok, nil}
  defp history(command, events), do: Timeline.build(command.tenant, command.run_id, events)

  defp fresh(command, run) do
    revision = if run, do: run.revision, else: 0

    cond do
      revision != command.expected_version -> {:error, :stale_revision}
      revision >= Timeline.limit() -> {:error, :run_too_large}
      true -> next(command, run)
    end
  end

  defp next(command, run) do
    with :ok <- Timeline.validate_next(run, command.payload) do
      {:append,
       %{
         "tenant_id" => command.tenant,
         "entity_id" => Event.entity(command.tenant, command.run_id),
         "event_type" => "agent_run.v1." <> command.payload["kind"],
         "payload" => command.payload,
         "expected_version" => command.expected_version,
         "metadata" => %{@metadata_key => command.command_sha256}
       }}
    end
  end

  defp recover(command, event) do
    if event["payload"] == command.payload and event["version"] == command.expected_version + 1 do
      {:existing, receipt(event, "already_recorded")}
    else
      {:error, :operation_conflict}
    end
  end

  def receipt(event, disposition) do
    %{
      event_id: event["id"],
      version: event["version"],
      timestamp: event["timestamp"],
      disposition: disposition,
      execution: "none",
      action_authority: "not_established"
    }
  end

  defp metadata_hash(%{"metadata" => metadata}) when is_map(metadata), do: metadata[@metadata_key]
  defp metadata_hash(_), do: nil
end
