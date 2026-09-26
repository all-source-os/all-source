defmodule QueryServiceEx.Infrastructure.Adapters.CustomerAgentAccessStore do
  @moduledoc """
  Uncached leader reads for the existing customer review access port.

  Control Plane persists its member list under team:<tenant>:members. QS's
  separate TeamStore and token-derived /me response are not substitutes. Missing
  membership is denied; no owner is inferred or provisioned from a tenant slug.
  """

  @behaviour QueryServiceEx.Domain.CustomerAgent.AccessPort

  alias QueryServiceEx.Infrastructure.Adapters.CustomerAgentGrantStore
  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient

  @impl true
  defdelegate verify_credential(token, binding, operation, now), to: CustomerAgentGrantStore

  @impl true
  def tenant(id) do
    case RustCoreClient.get_tenant_for_authorization(id) do
      {:ok, record} when is_map(record) -> {:ok, record}
      {:error, :not_found} -> {:error, :access_denied}
      _ -> {:error, :storage_unavailable}
    end
  rescue
    _ -> {:error, :storage_unavailable}
  end

  @impl true
  def members(tenant_id) do
    case RustCoreClient.get_team_members_for_authorization(tenant_id) do
      {:ok, members} when is_list(members) -> {:ok, members}
      {:error, :not_found} -> {:error, :access_denied}
      _ -> {:error, :storage_unavailable}
    end
  rescue
    _ -> {:error, :storage_unavailable}
  end
end
