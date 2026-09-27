defmodule QueryServiceEx.Domain.AgentRun.Event do
  @moduledoc """
  Metadata-only agent-run v1 evidence. Recorded approval is an observation, never
  product action authority. Raw prompts, tool arguments and source are excluded.
  """
  @keys ~w(schema_version run_id kind change_id change_number attempt_id causation_id
           agent_sha256 model_sha256 prompt_sha256 evidence_sha256 outcome)
  @run_kinds ~w(run.started run.completed capture_gap)
  @change_kinds ~w(change.proposed change.approved change.abandoned)
  @attempt_kinds ~w(attempt.started attempt.tested attempt.failed attempt.accepted
                   attempt.reverted attempt.unknown attempt.reconciled)
  @hashes ~w(agent_sha256 model_sha256 prompt_sha256 evidence_sha256)
  @max_integer 9_007_199_254_740_991

  def keys, do: @keys
  def kinds, do: @run_kinds ++ @change_kinds ++ @attempt_kinds

  def tenant?(value) when is_binary(value) and byte_size(value) in 1..128,
    do: Regex.match?(~r/\A[A-Za-z0-9_-]+\z/, value)

  def tenant?(_), do: false

  def entity(tenant, run_id) do
    hash = :sha256 |> :crypto.hash(tenant) |> Base.encode16(case: :lower)
    "agent-run-v1-" <> hash <> "-" <> run_id
  end

  def uuid?(value) when is_binary(value) and byte_size(value) == 36,
    do: Regex.match?(~r/\A[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\z/, value)

  def uuid?(_), do: false

  @doc "Validate before typed ingestion and again before reading generic stored events."
  def validate(payload) when is_map(payload) do
    if shape?(payload) and
         Enum.all?(@hashes, &(is_nil(payload[&1]) or hash?(payload[&1]))) and
         descriptors?(payload) and
         nullable_uuid?(payload["causation_id"]) and references?(payload) and
         outcome?(payload) do
      {:ok, payload}
    else
      {:error, :invalid_run_event}
    end
  end

  def validate(_), do: {:error, :invalid_run_event}

  defp shape?(payload),
    do:
      Enum.sort(Map.keys(payload)) == Enum.sort(@keys) and
        payload["schema_version"] === 1 and uuid?(payload["run_id"]) and
        payload["kind"] in kinds()

  @doc "Bind the stored envelope to the caller's tenant and requested run."
  def stored(event, tenant, run_id) when is_map(event) do
    with {:ok, payload} <- validate(event["payload"]),
         true <- payload["run_id"] == run_id and event["tenant_id"] == tenant,
         true <- event["entity_id"] == entity(tenant, run_id),
         true <- event["event_type"] == "agent_run.v1." <> payload["kind"],
         true <- uuid?(event["id"]) and positive?(event["version"]),
         true <- timestamp?(event["timestamp"]) do
      {:ok, Map.merge(payload, Map.take(event, ~w(id version timestamp)))}
    else
      _ -> {:error, :invalid_run_event}
    end
  end

  def stored(_, _, _), do: {:error, :invalid_run_event}

  defp references?(%{"kind" => kind} = value) when kind in @run_kinds do
    is_nil(value["change_id"]) and is_nil(value["change_number"]) and
      is_nil(value["attempt_id"]) and
      (kind != "run.started" or
         (is_nil(value["causation_id"]) and
            Enum.all?(~w(agent_sha256 model_sha256 prompt_sha256), &hash?(value[&1]))))
  end

  defp references?(%{"kind" => kind} = value) do
    uuid?(value["change_id"]) and positive?(value["change_number"]) and
      uuid?(value["causation_id"]) and
      if(kind in @change_kinds,
        do: is_nil(value["attempt_id"]),
        else: uuid?(value["attempt_id"])
      )
  end

  defp outcome?(%{"kind" => "attempt.tested", "outcome" => outcome, "evidence_sha256" => hash}),
    do: outcome in ~w(pass fail) and hash?(hash)

  defp outcome?(%{
         "kind" => "attempt.reconciled",
         "outcome" => outcome,
         "evidence_sha256" => hash
       }),
       do: outcome in ~w(succeeded failed) and hash?(hash)

  defp outcome?(%{"kind" => kind, "outcome" => nil, "evidence_sha256" => hash})
       when kind in ~w(change.proposed change.approved attempt.reverted),
       do: hash?(hash)

  defp outcome?(%{"outcome" => nil}), do: true
  defp outcome?(_), do: false

  defp descriptors?(%{"kind" => "run.started"}), do: true

  defp descriptors?(payload),
    do: Enum.all?(~w(agent_sha256 model_sha256 prompt_sha256), &is_nil(payload[&1]))

  defp nullable_uuid?(nil), do: true
  defp nullable_uuid?(value), do: uuid?(value)
  defp positive?(value), do: is_integer(value) and value in 1..@max_integer

  defp hash?(value) when is_binary(value) and byte_size(value) == 64,
    do: Regex.match?(~r/\A[0-9a-f]{64}\z/, value)

  defp hash?(_), do: false

  defp timestamp?(value) when is_binary(value) and byte_size(value) <= 40,
    do: match?({:ok, _, _}, DateTime.from_iso8601(value))

  defp timestamp?(_), do: false
end
