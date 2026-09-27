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
  @evidence_version "review-evidence-v2"
  @evidence_fields @fields ++ ~w(selected_run_metadata comparison_evidence pending_review_status)
  @evidence_operations @operations ++ ~w(prepare_proposal read_review read_result)
  @replay_version "review-replay-v3"
  @replay_fields @evidence_fields ++ ~w(selected_replay_analysis rebuild_plan action_result)

  def version, do: @version
  def fields, do: @fields
  def operations, do: @operations
  def clients, do: @clients
  def evidence_version, do: @evidence_version
  def evidence_fields, do: @evidence_fields
  def evidence_operations, do: @evidence_operations
  def replay_version, do: @replay_version
  def replay_fields, do: @replay_fields
  def evidence_versions, do: [@evidence_version, @replay_version]

  @spec receipt(term(), term(), term(), term()) :: {:ok, map()} | {:error, :invalid_consent}
  def receipt(client, operations, acceptance, now) do
    contract = contract(acceptance)

    if client in @clients and not is_nil(contract) and
         valid_operations?(operations, contract.operations) and
         acceptance == %{"accepted" => true, "version" => contract.version} and
         is_integer(now) and now >= 0 do
      {:ok,
       %{
         "version" => contract.version,
         "client_id" => client,
         "fields" => contract.fields,
         "operations" => operations,
         "accepted_at" => now
       }}
    else
      {:error, :invalid_consent}
    end
  end

  @spec valid?(term()) :: boolean()
  def valid?(%{"consent" => consent} = grant) do
    contract = contract(consent)

    expected = %{
      "version" => contract && contract.version,
      "client_id" => grant["client_id"],
      "fields" => contract && contract.fields,
      "operations" => grant["operations"],
      "accepted_at" => grant["created_at"]
    }

    not is_nil(contract) and grant["client_id"] in @clients and
      valid_operations?(grant["operations"], contract.operations) and
      consent == expected
  end

  def valid?(_), do: false

  defp contract(%{"version" => @version}),
    do: %{version: @version, fields: @fields, operations: @operations}

  defp contract(%{"version" => @evidence_version}),
    do: %{version: @evidence_version, fields: @evidence_fields, operations: @evidence_operations}

  defp contract(%{"version" => @replay_version}),
    do: %{version: @replay_version, fields: @replay_fields, operations: @evidence_operations}

  defp contract(_), do: nil

  defp valid_operations?(operations, allowed)
       when is_list(operations) and length(operations) in 1..5,
       do:
         Enum.all?(operations, &(&1 in allowed)) and
           length(Enum.uniq(operations)) == length(operations)

  defp valid_operations?(_, _), do: false
end
