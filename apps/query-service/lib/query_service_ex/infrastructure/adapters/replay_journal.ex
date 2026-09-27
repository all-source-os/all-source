defmodule QueryServiceEx.Infrastructure.Adapters.ReplayJournal do
  @moduledoc "Core-backed one-dispatch replay identity and immutable terminal evidence."
  alias QueryServiceEx.Domain.AgentRun.Event
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOwner, as: Owner
  alias QueryServiceEx.Infrastructure.Adapters.CoreConfigTransport, as: HTTP
  alias QueryServiceEx.Projections.ReplayRecord, as: Record

  def reserve(tenant, projection, operation) do
    with {:ok, record} <- Record.new(tenant, projection, operation) do
      case replace(record, nil) do
        :ok -> {:new, record}
        {:error, :conflict} -> existing(record)
        error -> error
      end
    end
  end

  def load(tenant, operation) do
    with {:ok, key} <- key(tenant, operation) do
      case HTTP.request(:get, "/api/v1/config/" <> key, nil) do
        {:ok, 404, _} ->
          {:error, :not_found}

        {:ok, 200, %{"key" => ^key, "value" => record, "revision" => revision}} ->
          if Event.uuid?(revision) and Record.valid?(record, tenant, operation),
            do: {:ok, record, revision},
            else: {:error, :storage_unavailable}

        _ ->
          {:error, :storage_unavailable}
      end
    end
  end

  def finish(record, outcome) when is_map(record) do
    with true <- Record.valid?(record, record["tenant_id"], record["operation_id"]),
         {:ok, next} <- Record.finish(record, outcome),
         {:ok, current, revision} <- load(record["tenant_id"], record["operation_id"]) do
      cond do
        current == next -> :ok
        Record.terminal?(current) -> {:error, :conflict}
        current != record -> {:error, :conflict}
        true -> finalize(next, revision)
      end
    else
      false -> {:error, :invalid_replay}
      error -> error
    end
  end

  def finish(_, _), do: {:error, :invalid_replay}

  defp finalize(next, revision) do
    case replace(next, revision) do
      :ok ->
        :ok

      error ->
        case load(next["tenant_id"], next["operation_id"]) do
          {:ok, ^next, _} -> :ok
          _ -> error
        end
    end
  end

  defp existing(expected) do
    with {:ok, record, _} <- load(expected["tenant_id"], expected["operation_id"]) do
      if Record.immutable(record) == Record.immutable(expected),
        do: {:existing, record},
        else: {:error, :operation_conflict}
    end
  end

  defp replace(record, revision) do
    with {:ok, key} <- key(record["tenant_id"], record["operation_id"]) do
      condition =
        if revision, do: %{kind: "revision", revision: revision}, else: %{kind: "absent"}

      body = %{key: key, value: record, condition: condition, changed_by: "tenant-replay-service"}

      case HTTP.request(:post, "/api/v1/config/conditional/set", body) do
        {:ok, 200, %{"key" => ^key, "saved" => true, "revision" => next}} ->
          if Event.uuid?(next) and next != revision, do: :ok, else: {:error, :storage_unavailable}

        {:ok, 409, %{"error" => "Concurrency error: Configuration precondition failed"}} ->
          {:error, :conflict}

        _ ->
          {:error, :storage_unavailable}
      end
    end
  end

  defp key(tenant, operation) do
    if Event.tenant?(tenant) and Event.uuid?(operation),
      do: {:ok, "tenant_replay_v1." <> Owner.digest([tenant, operation])},
      else: {:error, :invalid_replay}
  end
end
