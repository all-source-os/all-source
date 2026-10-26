defmodule QueryServiceEx.Infrastructure.Adapters.ApiKeyAuthority do
  @moduledoc "Current managed-key authority from Core's strict retained lifecycle stream."

  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient

  @scopes ~w(events:read events:write queries:execute projections:read projections:write schemas:read schemas:write)

  def kind(claims) do
    marker = claims["credential_kind"]

    cond do
      claims["is_api_key"] != true and marker == nil and claims["role"] != "serviceaccount" ->
        {:ok, :human}

      claims["is_api_key"] != true ->
        {:error, :invalid_key}

      marker == "service_delegation" ->
        delegation(claims)

      marker == nil and legacy_delegation?(claims) ->
        delegation(claims)

      marker in [nil, "managed_api_key"] ->
        {:ok, :managed}

      true ->
        {:error, :invalid_key}
    end
  end

  def authorize(claims) do
    with true <- valid_token_expiry?(claims),
         {:ok, :managed} <- kind(claims),
         {:ok, records} <- records(claims["tenant_id"]),
         %{} = key <- Map.get(records, claims["sub"]),
         true <- key["active"] == true,
         true <- valid_scopes?(key["scopes"]),
         true <- not non_expiring_token?(claims) or is_nil(key["expires_at"]),
         :ok <- expiry(key["expires_at"]) do
      {:ok, Map.put(key, "role", claims["role"])}
    else
      {:error, _} = error -> error
      _ -> {:error, :invalid_key}
    end
  end

  def records(tenant) when is_binary(tenant) and byte_size(tenant) in 1..128 do
    with {:ok, page} <- RustCoreClient.key_history_for_authorization(tenant) do
      fold(page, tenant)
    end
  end

  def records(_), do: {:error, :invalid_key}

  def non_expiring_token?(claims),
    do:
      claims["is_api_key"] == true and claims["credential_kind"] == "managed_api_key" and
        not Map.has_key?(claims, "exp")

  def valid_token_expiry?(claims),
    do:
      non_expiring_token?(claims) or
        (is_integer(claims["exp"]) and claims["exp"] > System.system_time(:second))

  def fold(page, tenant) do
    entity = "apikey:" <> tenant

    with %{
           "events" => events,
           "count" => count,
           "total_count" => count,
           "has_more" => false,
           "entity_version" => version,
           "archive_integrity" => %{
             "protocol" => "retained-entity-v1",
             "tenant_id" => ^tenant,
             "entity_id" => ^entity
           }
         } <- page,
         true <- is_list(events) and length(events) == count and count <= 1000,
         {:ok, {records, _ids, ^count, numbered, _timestamp}} <-
           Enum.reduce_while(events, {:ok, {%{}, MapSet.new(), 0, false, nil}}, fn event,
                                                                                   {:ok, state} ->
             case fold_event(event, tenant, entity, state) do
               {:ok, next} -> {:cont, {:ok, next}}
               _ -> {:halt, {:error, :invalid_key}}
             end
           end),
         true <- version === if(numbered, do: count, else: min(count, 1)) do
      {:ok, records}
    else
      _ -> {:error, :invalid_key}
    end
  end

  defp fold_event(event, tenant, entity, {records, ids, previous, numbered, last_timestamp}) do
    with %{
           "id" => id,
           "tenant_id" => ^tenant,
           "entity_id" => ^entity,
           "version" => version,
           "timestamp" => timestamp
         } <- event,
         true <- is_binary(id) and id != "" and not MapSet.member?(ids, id),
         true <-
           is_integer(version) and (version == previous + 1 or (not numbered and version == 1)),
         true <- is_binary(timestamp),
         {:ok, parsed, _} <- DateTime.from_iso8601(timestamp),
         true <- is_nil(last_timestamp) or DateTime.compare(parsed, last_timestamp) != :lt,
         {:ok, updated} <- apply_event(event, records, tenant) do
      {:ok, {updated, MapSet.put(ids, id), previous + 1, numbered or version > 1, parsed}}
    else
      _ -> {:error, :invalid_key}
    end
  end

  defp apply_event(
         %{"event_type" => "apikey.created", "payload" => %{"key" => key}},
         records,
         tenant
       )
       when is_map(key) do
    id = key["id"]

    if is_binary(id) and id != "" and key["tenant_id"] == tenant do
      current = records[id]

      cond do
        is_nil(current) -> {:ok, Map.put(records, id, key)}
        security_fields(current) == security_fields(key) -> {:ok, records}
        true -> {:ok, Map.put(records, id, Map.put(current, "active", false))}
      end
    else
      {:error, :invalid_key}
    end
  end

  defp apply_event(
         %{"event_type" => "apikey.revoked", "payload" => %{"key_id" => id}},
         records,
         _
       )
       when is_binary(id) and id != "" do
    key = records[id] || %{"id" => id}
    {:ok, Map.put(records, id, Map.put(key, "active", false))}
  end

  defp apply_event(_, _, _), do: {:error, :invalid_key}

  def valid_scopes?(scopes),
    do: is_list(scopes) and scopes != [] and Enum.all?(scopes, &(&1 in @scopes))

  def expiry(nil), do: :ok

  def expiry(value) when is_binary(value) do
    case DateTime.from_iso8601(value) do
      {:ok, expires, _} ->
        if DateTime.compare(expires, DateTime.utc_now()) == :gt,
          do: :ok,
          else: {:error, :key_expired}

      _ ->
        {:error, :invalid_key}
    end
  end

  def expiry(_), do: {:error, :invalid_key}

  defp security_fields(key),
    do:
      key |> Map.take(~w(id tenant_id active scopes)) |> Map.put("expires_at", key["expires_at"])

  defp blank?(value), do: value in [nil, ""]

  defp legacy_delegation?(claims),
    do: blank?(claims["name"]) and blank?(claims["username"]) and short_lived?(claims)

  defp short_lived?(%{"iat" => iat, "exp" => exp}) when is_integer(iat) and is_integer(exp),
    do: iat > 0 and iat <= System.system_time(:second) + 5 and exp > iat and exp - iat <= 60

  defp short_lived?(_), do: false

  defp delegation(claims) do
    if claims["iss"] == "allsource" and short_lived?(claims) and
         blank?(claims["name"]) and blank?(claims["username"]),
       do: {:ok, :delegation},
       else: {:error, :invalid_key}
  end
end
