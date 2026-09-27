defmodule QueryServiceEx.Infrastructure.Adapters.CustomerAgentGrantStore do
  @moduledoc """
  Opaque credentials and versioned consent in a conditional Core tenant registry.

  Only hashes persist. Separate revocation markers survive stale registry restores.
  Callers must establish the human actor and current membership/entitlement.
  Verification alone never proves source or action authority. Pre-consent v1
  credentials are deliberately not accepted; reconnect is required.
  """
  alias QueryServiceEx.Domain.CustomerAgent.ConnectionConsent
  alias QueryServiceEx.Domain.CustomerAgent.ConnectionGrant
  alias QueryServiceEx.Domain.CustomerAgent.ConnectionRegistry
  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient

  @behaviour QueryServiceEx.Domain.CustomerAgent.ConnectionPort

  @prefix "asreview_v2_"
  @revoked_prefix "customer_agent_v1.revoked."
  @context_fields ~w(id tenant_id subject_id client_id resource version operations created_at expires_at active consent)

  @spec issue(map(), list(), map(), integer(), integer()) :: {:ok, map()} | {:error, atom()}
  @impl true
  def issue(binding, operations, acceptance, now, ttl) do
    with {:ok, record} <- ConnectionGrant.new(binding, operations, now, ttl),
         {:ok, consent} <-
           ConnectionConsent.receipt(binding["client_id"], operations, acceptance, now) do
      id = random_hex(16)
      token = @prefix <> id <> "." <> random_hex(32)

      stored =
        Map.merge(record, %{
          "id" => id,
          "token_hash" => digest(token),
          "consent" => consent,
          "revoked_at" => nil
        })

      with :ok <- update(binding["tenant_id"], &ConnectionRegistry.insert(&1, stored, now), 4) do
        {:ok, %{id: id, token: token, expires_at: stored["expires_at"]}}
      end
    end
  end

  @spec verify_credential(term(), term(), term(), term()) :: {:ok, map()} | {:error, atom()}
  def verify_credential(token, binding, operation, now) do
    with {:ok, record} <- verify_base(token, binding, operation, now),
         {:ok, "active"} <- activation_state(record, now) do
      {:ok, Map.take(record, @context_fields)}
    else
      {:error, :storage_unavailable} = error -> error
      _ -> {:error, :unauthorized}
    end
  end

  @doc "Consume a PKCE-validated remote authorization once. Replay revokes its credential."
  @impl true
  def activate_remote(token, binding, now) do
    with true <- is_map(binding) and binding["client_id"] == "claude-ai",
         {:ok, record} <- verify_base(token, binding, "read_context", now),
         true <- now < record["created_at"] + 300 do
      case RustCoreClient.activate_customer_remote_grant(record["id"], digest(token), now) do
        :ok ->
          :ok

        {:error, :conflict} ->
          case persist_marker(record["id"]) do
            :ok -> {:error, :unauthorized}
            error -> error
          end

        _ ->
          {:error, :storage_unavailable}
      end
    else
      {:error, :storage_unavailable} = error -> error
      _ -> {:error, :unauthorized}
    end
  rescue
    _ -> {:error, :storage_unavailable}
  end

  defp verify_base(token, binding, operation, now) do
    with true <- ConnectionGrant.valid_binding?(binding),
         {:ok, id} <- token_id(token),
         {:ok, record} <- fetch(binding["tenant_id"], id),
         true <- matches_secret?(record, token),
         true <- ConnectionConsent.valid?(record) and is_nil(record["revoked_at"]),
         true <- ConnectionGrant.valid_for?(record, binding, operation, now),
         :ok <- not_revoked(id) do
      {:ok, record}
    else
      {:error, :storage_unavailable} = error -> error
      _ -> {:error, :unauthorized}
    end
  end

  @doc "List one subject's minimal receipts without hashes or other members."
  @impl true
  def list(tenant, subject, now) do
    with true <- ConnectionGrant.valid_id?(tenant) and ConnectionGrant.valid_subject?(subject),
         true <- is_integer(now) and now >= 0,
         {:ok, registry, _revision} <- read_registry(tenant) do
      registry["grants"]
      |> Map.values()
      |> Enum.filter(&(&1["subject_id"] == subject))
      |> Enum.reduce_while({:ok, []}, fn record, {:ok, records} ->
        case summary(record, now) do
          {:ok, item} -> {:cont, {:ok, [item | records]}}
          error -> {:halt, error}
        end
      end)
    else
      {:error, :storage_unavailable} = error -> error
      _ -> {:error, :unauthorized}
    end
  end

  @spec revoke(term(), term(), term()) :: :ok | {:error, atom()}
  @impl true
  def revoke(binding, id, now) do
    with true <- ConnectionGrant.valid_binding?(binding) and ConnectionRegistry.valid_id?(id),
         true <- is_integer(now) and now >= 0,
         {:ok, record} <- fetch(binding["tenant_id"], id),
         true <- ConnectionGrant.matches_owner?(record, binding),
         :ok <- persist_marker(id) do
      update(binding["tenant_id"], &ConnectionRegistry.revoke(&1, id, now), 4)
    else
      {:error, :storage_unavailable} = error -> error
      _ -> {:error, :unauthorized}
    end
  end

  @doc "Read a receipt for server-side ownership and binding checks."
  @impl true
  def fetch(tenant, id) do
    with true <- ConnectionRegistry.valid_id?(id),
         {:ok, registry, _revision} <- read_registry(tenant),
         record when is_map(record) <- registry["grants"][id] do
      {:ok, record}
    else
      {:error, :storage_unavailable} = error -> error
      _ -> {:error, :unauthorized}
    end
  end

  defp read_registry(tenant) do
    case RustCoreClient.get_customer_connection_registry(tenant) do
      {:ok, value, revision} ->
        if ConnectionRegistry.valid?(value, tenant),
          do: {:ok, value, revision},
          else: {:error, :storage_unavailable}

      {:error, :not_found} ->
        {:ok, ConnectionRegistry.empty(tenant), nil}

      _ ->
        {:error, :storage_unavailable}
    end
  rescue
    _ -> {:error, :storage_unavailable}
  end

  defp update(_tenant, _change, 0), do: {:error, :storage_unavailable}

  defp update(tenant, change, attempts) do
    with {:ok, registry, revision} <- read_registry(tenant),
         {:ok, next} <- change.(registry) do
      case RustCoreClient.put_customer_connection_registry(tenant, next, revision) do
        :ok -> :ok
        {:error, :conflict} -> update(tenant, change, attempts - 1)
        _ -> {:error, :storage_unavailable}
      end
    end
  rescue
    _ -> {:error, :storage_unavailable}
  end

  defp persist_marker(id) do
    case RustCoreClient.put_config_for_authorization(@revoked_prefix <> id, %{"revoked" => true}) do
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

  defp summary(record, now) do
    with {:ok, status} <- status(record, now) do
      {:ok,
       record
       |> Map.take(~w(id client_id resource operations created_at expires_at consent))
       |> Map.put("status", status)}
    end
  end

  defp status(record, now) do
    cond do
      not is_nil(record["revoked_at"]) ->
        {:ok, "revoked"}

      record["expires_at"] <= now ->
        {:ok, "expired"}

      true ->
        case not_revoked(record["id"]) do
          :ok -> activation_state(record, now)
          {:error, :unauthorized} -> {:ok, "revoked"}
          error -> error
        end
    end
  end

  defp activation_state(%{"client_id" => "claude-code"}, _now), do: {:ok, "active"}

  defp activation_state(%{"client_id" => "claude-ai"} = record, now) do
    case RustCoreClient.get_config_for_authorization(
           "customer_agent_v2.remote_redeemed." <> record["id"]
         ) do
      {:error, :not_found} ->
        {:ok, "pending"}

      {:ok, %{"version" => 1, "token_hash" => hash, "redeemed_at" => stamp} = receipt}
      when map_size(receipt) == 3 and is_integer(stamp) ->
        if hash == record["token_hash"] and stamp >= record["created_at"] and
             stamp < record["created_at"] + 300 and stamp <= now,
           do: {:ok, "active"},
           else: {:error, :storage_unavailable}

      _ ->
        {:error, :storage_unavailable}
    end
  end

  defp activation_state(_, _), do: {:error, :unauthorized}

  defp matches_secret?(%{"token_hash" => hash}, token),
    do: Plug.Crypto.secure_compare(hash, digest(token))

  defp token_id(@prefix <> rest) when byte_size(rest) == 97 do
    case String.split(rest, ".", parts: 2) do
      [id, secret] ->
        if ConnectionRegistry.valid_id?(id) and Regex.match?(~r/\A[0-9a-f]{64}\z/, secret),
          do: {:ok, id},
          else: {:error, :unauthorized}

      _ ->
        {:error, :unauthorized}
    end
  end

  defp token_id(_), do: {:error, :unauthorized}
  defp random_hex(bytes), do: bytes |> :crypto.strong_rand_bytes() |> Base.encode16(case: :lower)
  defp digest(token), do: :sha256 |> :crypto.hash(token) |> Base.encode16(case: :lower)
end
