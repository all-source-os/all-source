defmodule QueryServiceEx.Infrastructure.Adapters.CustomerActionStore do
  @moduledoc "Bounded Core CAS registry for pending rebuild reviews and single-operation decisions."
  alias QueryServiceEx.Domain.AgentRun.Event
  alias QueryServiceEx.Domain.CustomerAgent.ReplayReview
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOwner, as: Owner
  alias QueryServiceEx.Infrastructure.Adapters.CoreConfigTransport, as: HTTP

  def fetch(tenant, id) do
    with true <- Owner.id?(id), {:ok, records, _} <- load(tenant) do
      case records[id] do
        nil -> {:error, :not_found}
        record -> {:ok, record}
      end
    else
      false -> {:error, :not_found}
      error -> error
    end
  end

  def insert(tenant, record, now) do
    change(
      tenant,
      record["id"],
      fn
        nil ->
          {:ok, record}

        existing ->
          if Owner.matches?(existing, record) and
               existing["request_sha256"] == record["request_sha256"],
             do: {:ok, existing},
             else: {:error, :review_conflict}
      end,
      now
    )
  end

  def change(tenant, id, fun, now), do: change(tenant, id, fun, now, 4)
  defp change(_, _, _, _, 0), do: {:error, :storage_unavailable}

  defp change(tenant, id, fun, now, remaining) do
    with true <- Owner.id?(id) and Owner.timestamp?(now),
         {:ok, records, revision} <- load(tenant),
         {:ok, next} <- fun.(records[id]),
         true <- ReplayReview.valid?(next, tenant) and next["id"] == id,
         kept =
           Map.reject(records, fn {key, value} ->
             key != id and value["expires_at"] <= now - 86_400
           end),
         true <- map_size(kept) < 32 or Map.has_key?(kept, id) do
      if next == records[id] do
        {:ok, next}
      else
        case replace(tenant, Map.put(kept, id, next), revision) do
          :ok -> {:ok, next}
          {:error, :conflict} -> change(tenant, id, fun, now, remaining - 1)
          error -> error
        end
      end
    else
      false -> {:error, :workspace_limit}
      error -> error
    end
  end

  def load(tenant) do
    with {:ok, key} <- key(tenant) do
      case HTTP.request(:get, "/api/v1/config/" <> key, nil) do
        {:ok, 404, _} ->
          {:ok, %{}, nil}

        {:ok, 200, %{"key" => ^key, "value" => value, "revision" => revision}} ->
          if Event.uuid?(revision) and valid?(value, tenant),
            do: {:ok, value["reviews"], revision},
            else: {:error, :storage_unavailable}

        _ ->
          {:error, :storage_unavailable}
      end
    end
  end

  defp replace(tenant, records, revision) do
    value = %{"schema_version" => 1, "tenant_id" => tenant, "reviews" => records}

    with {:ok, key} <- key(tenant),
         true <- valid?(value, tenant),
         true <- byte_size(Jason.encode!(value)) <= 60_000 do
      condition =
        if revision, do: %{kind: "revision", revision: revision}, else: %{kind: "absent"}

      body = %{
        key: key,
        value: value,
        condition: condition,
        changed_by: "customer-action-service"
      }

      case HTTP.request(:post, "/api/v1/config/conditional/set", body) do
        {:ok, 200, %{"key" => ^key, "saved" => true, "revision" => next}} ->
          if Event.uuid?(next) and next != revision, do: :ok, else: {:error, :storage_unavailable}

        {:ok, 409, %{"error" => "Concurrency error: Configuration precondition failed"}} ->
          {:error, :conflict}

        _ ->
          {:error, :storage_unavailable}
      end
    else
      false -> {:error, :workspace_limit}
      error -> error
    end
  end

  defp valid?(
         %{"schema_version" => 1, "tenant_id" => tenant, "reviews" => records} = value,
         tenant
       )
       when map_size(value) == 3 and is_map(records) and map_size(records) <= 32,
       do:
         Enum.all?(records, fn {id, record} ->
           ReplayReview.valid?(record, tenant) and record["id"] == id
         end)

  defp valid?(_, _), do: false

  defp key(tenant),
    do:
      if(Event.tenant?(tenant),
        do: {:ok, "customer_action_v1." <> Owner.digest(tenant)},
        else: {:error, :access_denied}
      )
end
