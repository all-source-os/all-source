defmodule QueryServiceEx.Application.Services.CustomerReviewRecords do
  @moduledoc "Internal conditional record operations; caller establishes owner and consent before use."
  alias QueryServiceEx.Domain.CustomerAgent.ReviewWorkspace

  def insert(tenant, kind, record, now), do: insert(tenant, kind, record, now, 4)
  defp insert(_, _, _, _, 0), do: {:error, :storage_unavailable}

  defp insert(tenant, kind, record, now, attempts) do
    with :ok <- store().active?(tenant, kind, record["id"]),
         {:ok, current, revision} <- store().load(tenant),
         {:ok, next} <- ReviewWorkspace.insert(current, kind, record, now) do
      case if(next == current, do: :ok, else: store().replace(tenant, next, revision)) do
        :ok ->
          with :ok <- store().active?(tenant, kind, record["id"]),
               do: {:ok, next[kind][record["id"]]}

        {:error, :conflict} ->
          insert(tenant, kind, record, now, attempts - 1)

        _ ->
          {:error, :storage_unavailable}
      end
    end
  end

  def fetch(tenant, kind, id) when kind in ~w(sources reviews) do
    with :ok <- store().active?(tenant, kind, id),
         {:ok, registry, _revision} <- store().load(tenant),
         record when is_map(record) <- registry[kind][id],
         :ok <- store().active?(tenant, kind, id) do
      {:ok, record}
    else
      nil -> {:error, :not_found}
      error -> error
    end
  end

  def revoke(tenant, kind, id), do: store().revoke(tenant, kind, id)
  def active?(tenant, kind, id), do: store().active?(tenant, kind, id)
  defp store, do: Application.fetch_env!(:query_service_ex, :customer_review_store)
end
