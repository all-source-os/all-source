defmodule QueryServiceEx.Projections.ReplayRecord do
  @moduledoc "Minimal execution metadata. This record is not human approval or source integrity evidence."
  alias QueryServiceEx.Domain.AgentRun.Event
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOwner, as: Owner
  alias QueryServiceEx.Projections.Catalog

  @protocol "tenant-replay-v1"
  @fields ~w(protocol tenant_id operation_id replay_id projection_name request_sha256 cutoff status completed_at processed_events)
  @terminal ~w(completed failed cancelled)

  def new(tenant, projection, operation) do
    if Event.tenant?(tenant) and Event.uuid?(operation) and Catalog.valid?(projection) do
      {:ok,
       %{
         "protocol" => @protocol,
         "tenant_id" => tenant,
         "operation_id" => operation,
         "replay_id" => "tracked_" <> Owner.digest([tenant, operation]),
         "projection_name" => projection,
         "request_sha256" => Owner.digest([@protocol, tenant, operation, projection]),
         "cutoff" => DateTime.utc_now() |> DateTime.to_iso8601(),
         "status" => "unknown",
         "completed_at" => nil,
         "processed_events" => nil
       }}
    else
      {:error, :invalid_replay}
    end
  end

  def valid?(value, tenant, operation) when is_map(value) do
    with true <- Enum.sort(Map.keys(value)) == Enum.sort(@fields),
         {:ok, expected} <- new(tenant, value["projection_name"], operation) do
      immutable(value) == immutable(expected) and timestamp?(value["cutoff"]) and
        valid_outcome?(value)
    else
      _ -> false
    end
  end

  def valid?(_, _, _), do: false

  def immutable(value),
    do: Map.take(value, @fields -- ~w(cutoff status completed_at processed_events))

  def finish(record, outcome) when is_map(outcome) do
    next = Map.merge(record, Map.take(outcome, ~w(status completed_at processed_events)))

    if Enum.sort(Map.keys(outcome)) == ~w(completed_at processed_events status) and
         terminal?(next) and valid_outcome?(next),
       do: {:ok, next},
       else: {:error, :invalid_replay}
  end

  def finish(_, _), do: {:error, :invalid_replay}
  def terminal?(record), do: record["status"] in @terminal
  def public(record), do: Map.drop(record, ["tenant_id", "protocol"])

  defp valid_outcome?(%{"status" => "unknown", "completed_at" => nil, "processed_events" => nil}),
    do: true

  defp valid_outcome?(%{"status" => status, "completed_at" => at, "processed_events" => count}),
    do: status in @terminal and timestamp?(at) and is_integer(count) and count in 0..10_000_000

  defp valid_outcome?(_), do: false

  defp timestamp?(value) when is_binary(value) and byte_size(value) <= 40,
    do: match?({:ok, _, 0}, DateTime.from_iso8601(value))

  defp timestamp?(_), do: false
end
