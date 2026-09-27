defmodule QueryServiceEx.Domain.CustomerAgent.QueryOperationJournal do
  @moduledoc "Bounded original admission requests, not another billing counter. Expired IDs cannot be reused after pruning."
  alias QueryServiceEx.Domain.AgentRun.Event
  alias QueryServiceEx.Domain.CustomerAgent.QueryAdmission
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOperation, as: Operation
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOwner, as: Owner
  @limit 192

  def empty(tenant),
    do: %{"schema_version" => 1, "tenant_id" => tenant, "updated_at" => 0, "operations" => %{}}

  def valid?(value, tenant) when is_map(value) do
    Enum.sort(Map.keys(value)) == ~w(operations schema_version tenant_id updated_at) and
      value["schema_version"] === 1 and Event.tenant?(tenant) and value["tenant_id"] == tenant and
      Owner.timestamp?(value["updated_at"]) and is_map(value["operations"]) and
      map_size(value["operations"]) <= @limit and
      Enum.all?(value["operations"], fn {id, request} ->
        valid_entry?(id, request, value["updated_at"])
      end) and byte_size(Jason.encode!(value)) <= 60_000
  end

  def valid?(_, _), do: false

  def find(journal, base) do
    case journal["operations"][base["operation_id"]] do
      nil ->
        {:ok, nil}

      entry ->
        if Map.delete(entry, "expected_period") == base,
          do: {:ok, entry},
          else: {:error, :idempotency_conflict}
    end
  end

  def insert(journal, request, now) do
    entries =
      Map.reject(journal["operations"], fn {id, _} -> not Operation.valid_at?(id, now) end)

    cond do
      not Owner.timestamp?(now) or now < journal["updated_at"] ->
        {:error, :clock_moved_backwards}

      not QueryAdmission.request?(request, now) ->
        {:error, :invalid_operation}

      map_size(entries) >= @limit ->
        {:error, :query_operation_capacity}

      true ->
        next = %{
          journal
          | "updated_at" => now,
            "operations" => Map.put(entries, request["operation_id"], request)
        }

        if valid?(next, journal["tenant_id"]),
          do: {:ok, next},
          else: {:error, :invalid_operation}
    end
  end

  defp valid_entry?(id, request, updated) do
    case Operation.issued_at(id) do
      {:ok, issued} ->
        is_map(request) and request["operation_id"] == id and issued <= updated and
          QueryAdmission.request?(request, issued)

      _ ->
        false
    end
  end
end
