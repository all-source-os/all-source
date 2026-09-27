defmodule QueryServiceEx.Projections.TrackedReplays do
  @moduledoc """
  Internal idempotent dispatch over the existing tenant replay engine. Callers
  must separately enforce human authority, approved inputs and quotas before
  using this foundation. No HTTP or MCP binding is provided here.
  """
  alias QueryServiceEx.Infrastructure.Adapters.ReplayJournal, as: Journal
  alias QueryServiceEx.Projections.ReplayRecord, as: Record
  alias QueryServiceEx.Projections.TenantProjections

  def start(tenant, projection, operation) do
    case Journal.reserve(tenant, projection, operation) do
      {:new, record} -> dispatch(record)
      {:existing, record} -> recover(record)
      error -> error
    end
  end

  def get(tenant, operation) do
    with {:ok, record, _} <- Journal.load(tenant, operation), do: recover(record)
  end

  def persist(record, replay) do
    Journal.finish(record, %{
      "status" => replay.status,
      "completed_at" => replay.completed_at,
      "processed_events" => replay.processed_events
    })
  end

  defp dispatch(record) do
    case TenantProjections.rebuild_tracked(record) do
      {:ok, _} -> recover(record)
      {:error, _} -> fail_before_dispatch(record)
    end
  catch
    :exit, _ -> {:ok, Record.public(record)}
  end

  defp fail_before_dispatch(record) do
    outcome = %{
      "status" => "failed",
      "completed_at" => DateTime.utc_now() |> DateTime.to_iso8601(),
      "processed_events" => 0
    }

    with :ok <- Journal.finish(record, outcome),
         {:ok, next} <- Record.finish(record, outcome),
         do: {:ok, Record.public(next)}
  end

  defp recover(%{"status" => status} = record) when status != "unknown",
    do: {:ok, Record.public(record)}

  defp recover(record) do
    case TenantProjections.get_replay(record["tenant_id"], record["replay_id"]) do
      {:ok, %{status: "running"}} ->
        {:ok, record |> Record.public() |> Map.put("status", "running")}

      {:ok, replay} ->
        with :ok <- persist(record, replay),
             {:ok, current, _} <- Journal.load(record["tenant_id"], record["operation_id"]),
             do: {:ok, Record.public(current)}

      {:error, :not_found} ->
        {:ok, Record.public(record)}
    end
  catch
    :exit, _ -> {:ok, Record.public(record)}
  end
end
