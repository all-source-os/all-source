defmodule QueryServiceEx.Application.Services.CustomerEvidenceSources do
  @moduledoc "Source selection by a verified product actor and resolution under a current scoped connection. No transport binding yet."
  alias QueryServiceEx.Application.Services.AgentRunEvidence
  alias QueryServiceEx.Application.Services.CustomerConnections
  alias QueryServiceEx.Application.Services.CustomerReviewDeadline
  alias QueryServiceEx.Application.Services.CustomerReviewRecords, as: Records
  alias QueryServiceEx.Domain.CustomerAgent.EvidenceSource
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOwner, as: Owner

  @consent %{"accepted" => true, "version" => "selected-run-evidence-v1"}

  def share(actor, connection, input, now) do
    CustomerReviewDeadline.run(fn -> do_share(actor, connection, input, now) end)
  end

  defp do_share(actor, connection, input, now) do
    with true <-
           is_map(input) and
             Enum.sort(Map.keys(input)) == ~w(consent operation_id revision run_id sha256 ttl),
         true <- input["consent"] == @consent,
         {:ok, owner} <- CustomerConnections.evidence_owner(actor, connection, now),
         {:ok, run} <- AgentRunEvidence.read(owner["tenant_id"], input["run_id"]),
         true <- run.revision === input["revision"] and run.digest == input["sha256"],
         owner = cap_expiry(owner),
         {:ok, source} <- EvidenceSource.new(owner, run, input["operation_id"], now, input["ttl"]),
         {:ok, stored} <- Records.insert(owner["tenant_id"], "sources", source, now),
         {:ok, _} <-
           CustomerConnections.evidence_owner(actor, connection, System.system_time(:second)),
         :ok <-
           EvidenceSource.authorize(
             stored,
             owner,
             EvidenceSource.reference(stored),
             System.system_time(:second)
           ) do
      {:ok, %{source: EvidenceSource.reference(stored), expires_at: stored["expires_at"]}}
    else
      false -> {:error, :invalid_source_request}
      {:error, _} = error -> error
      _ -> {:error, :storage_unavailable}
    end
  rescue
    _ -> {:error, :storage_unavailable}
  end

  def recheck(owner, reference, now) do
    with {:ok, source} <- Records.fetch(owner["tenant_id"], "sources", reference["ref"]) do
      EvidenceSource.authorize(source, owner, reference, now)
    end
  end

  def resolve(owner, reference, now) do
    with true <- is_map(reference) and Owner.id?(reference["ref"]),
         {:ok, source} <- Records.fetch(owner["tenant_id"], "sources", reference["ref"]),
         :ok <- EvidenceSource.authorize(source, owner, reference, now),
         {:ok, run} <- AgentRunEvidence.read(owner["tenant_id"], source["locator"]),
         true <- run.revision == source["revision"] and run.digest == source["sha256"],
         {:ok, ^source} <- Records.fetch(owner["tenant_id"], "sources", reference["ref"]),
         :ok <- EvidenceSource.authorize(source, owner, reference, System.system_time(:second)) do
      {:ok, run, source["expires_at"]}
    else
      false -> {:error, :source_changed}
      {:error, error} when error in [:not_found, :revoked] -> {:error, :source_denied}
      {:error, _} = error -> error
      _ -> {:error, :source_denied}
    end
  end

  defp cap_expiry(owner) do
    case owner["entitlement_expires_at"] do
      expiry when is_integer(expiry) -> Map.update!(owner, "grant_expires_at", &min(&1, expiry))
      _ -> owner
    end
  end
end
