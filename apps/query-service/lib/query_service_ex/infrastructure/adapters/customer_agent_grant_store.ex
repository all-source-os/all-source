defmodule QueryServiceEx.Infrastructure.Adapters.CustomerAgentGrantStore do
  @moduledoc """
  Core-backed opaque connection credentials with uncached leader verification.

  Existing ApiKeyStore uses a local read cache and cannot supply immediate
  cross-process revocation. This adapter stores minimal grants and separate
  revocation markers in existing admin-only Core system config records. Tenant
  metadata updates cannot overwrite them. It does not issue a user JWT or authorise
  customer data access: callers must separately check current membership,
  entitlement, consent and object ownership before and after returning evidence.
  Issuance/revocation are internal primitives, not customer-facing routes.
  """

  alias QueryServiceEx.Domain.CustomerAgent.ConnectionGrant
  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient

  @prefix "asreview_v1_"
  @grant_prefix "customer_agent_v1.grant."
  @revoked_prefix "customer_agent_v1.revoked."
  @context_fields ~w(id tenant_id subject_id client_id resource version operations created_at expires_at active)

  @doc "Persist a new server-authorised binding before returning its one-time secret."
  @spec issue(map(), list(), integer(), integer()) :: {:ok, map()} | {:error, atom()}
  def issue(binding, operations, now, ttl) do
    with {:ok, record} <- ConnectionGrant.new(binding, operations, now, ttl) do
      id = random_hex(16)
      token = @prefix <> id <> "." <> random_hex(32)
      stored = Map.merge(record, %{"id" => id, "token_hash" => digest(token)})

      case persist(@grant_prefix <> id, stored) do
        :ok -> {:ok, %{id: id, token: token, expires_at: stored["expires_at"]}}
        error -> error
      end
    end
  end

  @doc "Check only the credential; each call reads current metadata from Core's leader."
  @spec verify_credential(term(), term(), term(), term()) :: {:ok, map()} | {:error, atom()}
  def verify_credential(token, binding, operation, now) do
    with true <- ConnectionGrant.valid_binding?(binding),
         {:ok, id} <- token_id(token),
         {:ok, record} <- fetch(id),
         true <- matches_secret?(record, token),
         true <- ConnectionGrant.valid_for?(record, binding, operation, now),
         :ok <- not_revoked(id) do
      {:ok, Map.take(record, @context_fields)}
    else
      {:error, :storage_unavailable} = error -> error
      _ -> {:error, :unauthorized}
    end
  end

  @doc "Persist revocation; a failed write never becomes a local-only success."
  @spec revoke(term(), term(), term()) :: :ok | {:error, atom()}
  def revoke(binding, id, now) do
    with true <- ConnectionGrant.valid_binding?(binding) and valid_grant_id?(id),
         true <- is_integer(now) and now >= 0,
         {:ok, record} <- fetch(id),
         true <- ConnectionGrant.matches_owner?(record, binding) do
      persist(@revoked_prefix <> id, %{"revoked" => true})
    else
      {:error, :storage_unavailable} = error -> error
      _ -> {:error, :unauthorized}
    end
  end

  defp fetch(id) do
    case RustCoreClient.get_config_for_authorization(@grant_prefix <> id) do
      {:ok, record} when is_map(record) ->
        if record["id"] == id,
          do: {:ok, record},
          else: {:error, :unauthorized}

      {:error, :not_found} ->
        {:error, :unauthorized}

      _ ->
        {:error, :storage_unavailable}
    end
  rescue
    _ -> {:error, :storage_unavailable}
  end

  defp persist(key, record) do
    case RustCoreClient.put_config_for_authorization(key, record) do
      {:ok, _} -> :ok
      _ -> {:error, :storage_unavailable}
    end
  rescue
    _ -> {:error, :storage_unavailable}
  end

  defp not_revoked(id) do
    case RustCoreClient.get_config_for_authorization(@revoked_prefix <> id) do
      {:error, :not_found} -> :ok
      {:ok, _} -> {:error, :unauthorized}
      _ -> {:error, :storage_unavailable}
    end
  rescue
    _ -> {:error, :storage_unavailable}
  end

  defp matches_secret?(%{"token_hash" => hash}, token)
       when is_binary(hash) and byte_size(hash) == 64,
       do: Plug.Crypto.secure_compare(hash, digest(token))

  defp matches_secret?(_, _), do: false

  defp token_id(@prefix <> rest) when byte_size(rest) == 97 do
    case String.split(rest, ".", parts: 2) do
      [id, secret] ->
        if valid_grant_id?(id) and Regex.match?(~r/\A[0-9a-f]{64}\z/, secret),
          do: {:ok, id},
          else: {:error, :unauthorized}

      _ ->
        {:error, :unauthorized}
    end
  end

  defp token_id(_), do: {:error, :unauthorized}

  defp valid_grant_id?(id) when is_binary(id) and byte_size(id) == 32,
    do: Regex.match?(~r/\A[0-9a-f]{32}\z/, id)

  defp valid_grant_id?(_), do: false
  defp random_hex(bytes), do: bytes |> :crypto.strong_rand_bytes() |> Base.encode16(case: :lower)
  defp digest(token), do: :sha256 |> :crypto.hash(token) |> Base.encode16(case: :lower)
end
