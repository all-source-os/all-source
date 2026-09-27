defmodule QueryServiceEx.Domain.CustomerAgent.ConnectionRegistry do
  @moduledoc """
  Bounded tenant connection state committed through a single Core revision.

  Sixteen live connections and 64 issuances per rolling day are abuse bounds,
  not commercial plan limits. Revoked/expired receipts count for the whole day.
  Pruning the current view does not erase Core's immutable system history.
  """
  alias QueryServiceEx.Domain.CustomerAgent.ConnectionConsent
  alias QueryServiceEx.Domain.CustomerAgent.ConnectionGrant

  @day 86_400
  @record_keys ~w(id token_hash consent revoked_at tenant_id subject_id client_id resource version operations created_at expires_at active)

  def empty(tenant),
    do: %{"version" => 2, "tenant_id" => tenant, "updated_at" => 0, "grants" => %{}}

  @spec valid?(term(), term()) :: boolean()
  def valid?(%{"version" => 2, "tenant_id" => tenant, "grants" => grants} = registry, tenant)
      when is_map(grants) and map_size(grants) <= 64 do
    Enum.sort(Map.keys(registry)) == ~w(grants tenant_id updated_at version) and
      timestamp?(registry["updated_at"]) and
      Enum.all?(grants, fn {id, grant} -> valid_record?(id, grant, registry) end)
  end

  def valid?(_, _), do: false

  @spec insert(map(), map(), integer()) :: {:ok, map()} | {:error, atom()}
  def insert(registry, grant, now) do
    grants =
      Map.reject(registry["grants"], fn {_id, item} -> item["created_at"] <= now - @day end)

    live =
      Enum.count(grants, fn {_id, g} -> is_nil(g["revoked_at"]) and g["expires_at"] > now end)

    cond do
      now < registry["updated_at"] ->
        {:error, :clock_moved_backwards}

      Map.has_key?(grants, grant["id"]) ->
        {:error, :storage_unavailable}

      live >= 16 or map_size(grants) >= 64 ->
        {:error, :connection_limit}

      true ->
        {:ok, %{registry | "grants" => Map.put(grants, grant["id"], grant), "updated_at" => now}}
    end
  end

  @spec revoke(map(), String.t(), integer()) :: {:ok, map()} | {:error, atom()}
  def revoke(registry, id, now) do
    case registry["grants"][id] do
      nil ->
        {:error, :unauthorized}

      record ->
        stamp = max(now, registry["updated_at"])
        record = Map.put(record, "revoked_at", record["revoked_at"] || stamp)

        {:ok,
         %{registry | "grants" => Map.put(registry["grants"], id, record), "updated_at" => stamp}}
    end
  end

  def valid_id?(id) when is_binary(id) and byte_size(id) == 32,
    do: Regex.match?(~r/\A[0-9a-f]{32}\z/, id)

  def valid_id?(_), do: false

  defp valid_record?(id, %{"operations" => operations} = grant, registry)
       when is_list(operations) do
    binding = Map.take(grant, ~w(tenant_id subject_id client_id resource))

    Enum.sort(Map.keys(grant)) == Enum.sort(@record_keys) and valid_id?(id) and
      grant["id"] == id and grant["tenant_id"] == registry["tenant_id"] and
      ConnectionGrant.valid_for?(grant, binding, List.first(operations), grant["created_at"]) and
      grant["created_at"] <= registry["updated_at"] and
      ConnectionConsent.valid?(grant) and valid_hash?(grant["token_hash"]) and
      valid_revocation?(grant["revoked_at"], grant["created_at"], registry["updated_at"])
  end

  defp valid_record?(_, _, _), do: false
  defp valid_revocation?(nil, _created, _updated), do: true

  defp valid_revocation?(revoked, created, updated),
    do: timestamp?(revoked) and revoked >= created and revoked <= updated

  defp valid_hash?(hash) when is_binary(hash), do: Regex.match?(~r/\A[0-9a-f]{64}\z/, hash)
  defp valid_hash?(_), do: false
  defp timestamp?(value), do: is_integer(value) and value >= 0
end
