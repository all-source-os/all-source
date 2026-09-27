defmodule QueryServiceEx.Application.Services.CustomerQueryAdmission do
  @moduledoc "Persist an exact operation before admitting its canonical query units. Caller establishes current access first."
  alias QueryServiceEx.Domain.CustomerAgent.QueryOperationJournal, as: Journal
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOperation, as: Operation

  def admit(owner, purpose, operation, intent, count, now) do
    with {:ok, base} <- Operation.request(owner, purpose, operation, intent, count, now),
         {:ok, request} <- reserve(owner["tenant_id"], base, now, 4),
         {:ok, _receipt} <- meter().admit(owner["tenant_id"], request) do
      :ok
    end
  rescue
    _ -> {:error, :query_usage_unavailable}
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
