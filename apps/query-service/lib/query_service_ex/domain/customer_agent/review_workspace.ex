defmodule QueryServiceEx.Domain.CustomerAgent.ReviewWorkspace do
  @moduledoc "Bounded review metadata in one conditional Core record; pruning does not erase immutable history."
  alias QueryServiceEx.Domain.CustomerAgent.ConnectionGrant
  alias QueryServiceEx.Domain.CustomerAgent.EvidenceSource
  alias QueryServiceEx.Domain.CustomerAgent.PendingReview
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOwner, as: Owner
  @day 86_400
  @limits %{"sources" => 64, "reviews" => 32}

  def empty(tenant),
    do: %{
      "schema_version" => 1,
      "tenant_id" => tenant,
      "updated_at" => 0,
      "sources" => %{},
      "reviews" => %{}
    }

  def valid?(value, tenant) when is_map(value) do
    Enum.sort(Map.keys(value)) == ~w(reviews schema_version sources tenant_id updated_at) and
      value["schema_version"] === 1 and value["tenant_id"] == tenant and
      ConnectionGrant.valid_id?(tenant) and Owner.timestamp?(value["updated_at"]) and
      collection?(value, "sources", &EvidenceSource.valid?/2) and
      collection?(value, "reviews", &PendingReview.valid?/2)
  end

  def valid?(_, _), do: false

  def insert(registry, kind, record, now) when kind in ~w(sources reviews) do
    valid_record =
      if kind == "sources",
        do: EvidenceSource.valid?(record, registry["tenant_id"]),
        else: PendingReview.valid?(record, registry["tenant_id"])

    if valid_record and Owner.timestamp?(now),
      do: insert_valid(registry, kind, record, now),
      else: {:error, :invalid_review_record}
  end

  defp insert_valid(registry, kind, record, now) do
    records = Map.reject(registry[kind], fn {_id, item} -> item["created_at"] <= now - @day end)
    existing = registry[kind][record["id"]]

    cond do
      not Owner.timestamp?(now) or now < registry["updated_at"] ->
        {:error, :clock_moved_backwards}

      existing && same_request?(existing, record) ->
        {:ok, registry}

      existing ->
        {:error, :idempotency_conflict}

      map_size(records) >= @limits[kind] ->
        {:error, :workspace_limit}

      true ->
        next =
          registry
          |> Map.put(kind, Map.put(records, record["id"], record))
          |> Map.put("updated_at", now)

        if valid?(next, registry["tenant_id"]),
          do: {:ok, next},
          else: {:error, :invalid_review_record}
    end
  end

  defp same_request?(left, right),
    do:
      Owner.matches?(left, right) and
        Map.drop(left, ~w(created_at expires_at digest)) ==
          Map.drop(right, ~w(created_at expires_at digest))

  defp collection?(registry, kind, valid) do
    entries = registry[kind]

    is_map(entries) and map_size(entries) <= @limits[kind] and
      Enum.all?(entries, fn {id, record} ->
        valid.(record, registry["tenant_id"]) and record["id"] == id and
          record["created_at"] <= registry["updated_at"]
      end)
  end
end
