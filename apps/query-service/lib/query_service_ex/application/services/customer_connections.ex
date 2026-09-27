defmodule QueryServiceEx.Application.Services.CustomerConnections do
  @moduledoc """
  Human-session connection management, separate from agent tool authority.

  Browser transport authenticates the actor. Current Core membership and billing
  determine issuance; expired billing never prevents an owner from revoking.
  Sequential checks are not an atomic membership/billing/credential transaction.
  """
  alias QueryServiceEx.Domain.CustomerAgent.ConnectionConsent
  alias QueryServiceEx.Domain.CustomerAgent.ConnectionGrant
  alias QueryServiceEx.Domain.CustomerAgent.Eligibility

  @spec create(map(), map(), integer()) :: {:ok, map()} | {:error, atom()}
  def create(actor, params, now) do
    with true <- Enum.sort(Map.keys(params)) == ~w(client_id consent operations ttl),
         # The remote host must obtain credentials through its PKCE flow, not
         # this one-time local credential form.
         true <- params["client_id"] == "claude-code",
         binding = binding(actor, params["client_id"]),
         true <- ConnectionGrant.valid_binding?(binding),
         {:ok, _} <- eligible(binding, now),
         {:ok, issued} <-
           connections().issue(
             binding,
             params["operations"],
             params["consent"],
             now,
             params["ttl"]
           ) do
      case eligible(binding, System.system_time(:second)) do
        {:ok, _} ->
          {:ok, Map.put(issued, :binding, binding)}

        _ ->
          # Lost eligibility never returns a secret. Even if this compensating
          # write fails, every tool still checks current membership/entitlement.
          connections().revoke(binding, issued.id, System.system_time(:second))
          {:error, :access_denied}
      end
    else
      false -> {:error, :invalid_request}
      error -> error
    end
  rescue
    _ -> {:error, :storage_unavailable}
  end

  def list(actor, now) do
    with :ok <- current_member(actor),
         {:ok, records} <- connections().list(actor["tenant_id"], actor["subject_id"], now),
         :ok <- current_member(actor) do
      {:ok, %{connections: records}}
    end
  rescue
    _ -> {:error, :storage_unavailable}
  end

  def revoke(actor, id, now) do
    with :ok <- current_member(actor),
         {:ok, record} <- connections().fetch(actor["tenant_id"], id),
         true <- record["subject_id"] == actor["subject_id"],
         binding = Map.take(record, ~w(tenant_id subject_id client_id resource)),
         :ok <- connections().revoke(binding, id, now) do
      {:ok, %{revoked: true}}
    else
      false -> {:error, :access_denied}
      error -> error
    end
  rescue
    _ -> {:error, :storage_unavailable}
  end

  @doc "Authorize source selection for an already-authenticated product actor, not an agent credential."
  def evidence_owner(actor, id, now) do
    with {:ok, record} <- connections().fetch(actor["tenant_id"], id),
         binding = binding(actor, record["client_id"]),
         true <- ConnectionGrant.matches_owner?(record, binding),
         true <- ConnectionConsent.valid?(record),
         true <- record["consent"]["version"] == ConnectionConsent.evidence_version(),
         true <- ConnectionGrant.valid_for?(record, binding, "prepare_proposal", now),
         {:ok, eligibility} <- metered_eligibility(binding, now),
         {:ok, receipts} <- connections().list(actor["tenant_id"], actor["subject_id"], now),
         [%{"status" => "active"}] <- Enum.filter(receipts, &(&1["id"] == id)) do
      {:ok,
       Map.merge(
         eligibility,
         Map.merge(binding, %{"grant_id" => id, "grant_expires_at" => record["expires_at"]})
       )}
    else
      {:error, :storage_unavailable} = error -> error
      _ -> {:error, :access_denied}
    end
  rescue
    _ -> {:error, :storage_unavailable}
  end

  defp current_member(actor) do
    with {:ok, members} <- access().members(actor["tenant_id"]) do
      case Enum.filter(members, &(&1["user_id"] == actor["subject_id"])) do
        [%{"role" => role}] when role in ~w(admin member) -> :ok
        _ -> {:error, :access_denied}
      end
    end
  end

  @doc false
  def eligible(binding, now) do
    with {:ok, tenant} <- access().tenant(binding["tenant_id"]),
         {:ok, members} <- access().members(binding["tenant_id"]) do
      Eligibility.check(tenant, members, binding, now)
    end
  end

  defp metered_eligibility(binding, now) do
    with {:ok, tenant} <- access().tenant(binding["tenant_id"]),
         {:ok, members} <- access().members(binding["tenant_id"]) do
      Eligibility.check_metered(tenant, members, binding, now)
    end
  end

  @doc false
  def binding(actor, client) do
    %{
      "tenant_id" => actor["tenant_id"],
      "subject_id" => actor["subject_id"],
      "client_id" => client,
      "resource" => Application.get_env(:query_service_ex, :customer_review_resource)
    }
  end

  defp access, do: Application.fetch_env!(:query_service_ex, :customer_agent_access_store)
  defp connections, do: Application.fetch_env!(:query_service_ex, :customer_connection_store)
end
