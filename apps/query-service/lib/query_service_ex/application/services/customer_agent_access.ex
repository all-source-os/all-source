defmodule QueryServiceEx.Application.Services.CustomerAgentAccess do
  @moduledoc """
  Verify a scoped credential against live membership and hosted MCP entitlement.

  Existing AuthPipeline trusts token claims and uses cached tenant paths. The
  customer review boundary instead re-reads Core through AccessPort on each call.
  A successful result does not establish consent, source ownership, a browser
  session or human authority. Those gates must precede any source disclosure.
  """

  alias QueryServiceEx.Domain.CustomerAgent.ConnectionGrant
  alias QueryServiceEx.Domain.CustomerAgent.Eligibility

  @spec verify(term(), term(), term(), term()) :: {:ok, map()} | {:error, atom()}
  def verify(token, binding, operation, now),
    do: verify(token, binding, operation, now, &Eligibility.check/4)

  @doc "Verifies current access only; metered services must obtain separate Core quota admission."
  def verify_metered(token, binding, operation, now),
    do: verify(token, binding, operation, now, &Eligibility.check_metered/4)

  defp verify(token, binding, operation, now, eligible) do
    started = System.monotonic_time(:millisecond)
    store = Application.fetch_env!(:query_service_ex, :customer_agent_access_store)

    with {:ok, grant} <- store.verify_credential(token, binding, operation, now),
         {:ok, tenant} <- store.tenant(grant["tenant_id"]),
         {:ok, members} <- store.members(grant["tenant_id"]),
         {:ok, _} <- eligible.(tenant, members, binding, now),
         # Do not return success if revocation happened during the other reads.
         {:ok, ^grant} <- store.verify_credential(token, binding, operation, now),
         finished = now + div(System.monotonic_time(:millisecond) - started, 1_000),
         true <- ConnectionGrant.valid_for?(grant, binding, operation, finished),
         {:ok, eligibility} <- eligible.(tenant, members, binding, finished) do
      {:ok,
       Map.merge(eligibility, %{
         "grant_id" => grant["id"],
         "tenant_id" => grant["tenant_id"],
         "subject_id" => grant["subject_id"],
         "client_id" => grant["client_id"],
         "resource" => grant["resource"],
         "operation" => operation,
         "granted_operations" => grant["operations"],
         "grant_expires_at" => grant["expires_at"],
         "consent_version" => grant["consent"]["version"],
         "verified_at" => finished
       })}
    else
      {:error, :storage_unavailable} = error -> error
      _ -> {:error, :access_denied}
    end
  rescue
    _ -> {:error, :storage_unavailable}
  end
end
