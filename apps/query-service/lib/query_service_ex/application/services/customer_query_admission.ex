defmodule QueryServiceEx.Application.Services.CustomerQueryAdmission do
  @moduledoc "Persist an exact operation before admitting its canonical query units. Caller establishes current access first."
  alias QueryServiceEx.Domain.CustomerAgent.QueryOperationJournal, as: Journal
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOperation, as: Operation

  def admit(owner, purpose, operation, intent, count, now) do
    with {:ok, _receipt, _} <- receipt(owner, purpose, operation, intent, count, now, nil),
         do: :ok
  end

  def execute_once(owner, purpose, operation, intent, now, window_end, read)
      when purpose in ["history.timeline", "history.evidence"] do
    case receipt(owner, purpose, operation, intent, 1, now, window_end) do
      {:ok, %{replayed: false, receipt: %{"operation_id" => id}}, reserved_end} ->
        result = read.(reserved_end)
        outcome = if match?({:ok, _}, result), do: "succeeded", else: "failed"
        with :ok <- finish(owner["tenant_id"], id, outcome, 4), do: result

      {:ok, %{replayed: true}, _} ->
        {:error, :already_processed}

      error ->
        error
    end
  end

  defp receipt(owner, purpose, operation, intent, count, now, end_us) do
    with {:ok, base} <- Operation.request(owner, purpose, operation, intent, count, now),
         base = if(is_nil(end_us), do: base, else: Map.put(base, "window_end_us", end_us)),
         {:ok, reserved} <- reserve(owner["tenant_id"], base, now, 4),
         {frozen, request} = Map.pop(reserved, "window_end_us"),
         {:ok, admission} <- meter().admit(owner["tenant_id"], request),
         do: {:ok, admission, frozen}
  rescue
    _ -> {:error, :query_usage_unavailable}
  end

  defp finish(_, _, _, 0), do: {:error, :query_usage_unavailable}

  defp finish(tenant, id, outcome, attempts) do
    with {:ok, journal, revision} <- store().load(tenant),
         entry when is_map(entry) <- journal["operations"][id] do
      next = put_in(journal, ["operations", id], Map.put(entry, "outcome", outcome))

      case store().replace(tenant, next, revision) do
        {:error, :conflict} -> finish(tenant, id, outcome, attempts - 1)
        other -> other
      end
    else
      _ -> {:error, :query_usage_unavailable}
    end
  end

  defp reserve(_, _, _, 0), do: {:error, :query_usage_unavailable}

  defp reserve(tenant, base, now, attempts) do
    with {:ok, journal, revision} <- store().load(tenant),
         {:ok, previous} <- Journal.find(journal, base) do
      if previous do
        {:ok, previous}
      else
        reserve_new(tenant, base, now, journal, revision, attempts)
      end
    end
  end

  defp reserve_new(tenant, base, now, journal, revision, attempts) do
    with {:ok, %{"period" => period}} <- meter().snapshot(tenant),
         request = Map.put(base, "expected_period", period),
         {:ok, next} <- Journal.insert(journal, request, now) do
      case store().replace(tenant, next, revision) do
        :ok -> {:ok, request}
        {:error, :conflict} -> reserve(tenant, base, now, attempts - 1)
        _ -> {:error, :query_usage_unavailable}
      end
    end
  end

  defp store, do: Application.fetch_env!(:query_service_ex, :customer_query_operation_store)
  defp meter, do: Application.fetch_env!(:query_service_ex, :customer_query_usage_store)
end
