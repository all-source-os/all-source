defmodule QueryServiceEx.Domain.CustomerAgent.ConnectionConsent do
  @moduledoc """
  Versioned, explicit disclosure consent for the restricted review profile.

  Consent identifies the selected host, not an attestation of that host or an
  approval to execute anything. Adding source data requires a new consent version.
  """
  @version "review-metadata-v1"
  @fields ~w(workspace_identity membership_role mcp_entitlement proposal_validation)
  @operations ~w(read_context validate_proposal)
  @clients ~w(claude-code claude-ai)

  def version, do: @version
  def fields, do: @fields
  def operations, do: @operations
  def clients, do: @clients

  @spec receipt(term(), term(), term(), term()) :: {:ok, map()} | {:error, :invalid_consent}
  def receipt(client, operations, acceptance, now) do
    if client in @clients and valid_operations?(operations) and
         acceptance == %{"accepted" => true, "version" => @version} and
         is_integer(now) and now >= 0 do
      {:ok,
       %{
         "version" => @version,
         "client_id" => client,
         "fields" => @fields,
         "operations" => operations,
         "accepted_at" => now
       }}
    else
      {:error, :invalid_consent}
    end
  end

  @spec valid?(term()) :: boolean()
  def valid?(%{"consent" => consent} = grant) do
    expected = %{
      "version" => @version,
      "client_id" => grant["client_id"],
      "fields" => @fields,
      "operations" => grant["operations"],
      "accepted_at" => grant["created_at"]
    }

    grant["client_id"] in @clients and valid_operations?(grant["operations"]) and
      consent == expected
  end

  def valid?(_), do: false

  defp valid_operations?(operations) when is_list(operations) and length(operations) in 1..2,
    do:
      Enum.all?(operations, &(&1 in @operations)) and
        length(Enum.uniq(operations)) == length(operations)

  defp valid_operations?(_), do: false
end
