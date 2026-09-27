defmodule QueryServiceEx.Application.Services.CustomerEvidenceSources do
  @moduledoc "Source selection by a verified product actor and resolution under a current scoped connection. No transport binding yet."
  alias QueryServiceEx.Application.Services.AgentRunEvidence
  alias QueryServiceEx.Application.Services.CustomerConnections
  alias QueryServiceEx.Application.Services.CustomerQueryAdmission, as: Admission
  alias QueryServiceEx.Application.Services.CustomerReviewDeadline
  alias QueryServiceEx.Application.Services.CustomerReviewRecords, as: Records
  alias QueryServiceEx.Domain.CustomerAgent.EvidenceSource, as: Source
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOwner, as: Owner

  @consent %{"accepted" => true, "version" => "selected-run-evidence-v1"}

  def share(actor, connection, input, now) do
    CustomerReviewDeadline.run(actor, fn -> do_share(actor, connection, input, now) end)
  end

  defp do_share(actor, connection, input, now) do
    with true <-
           is_map(input) and
             Enum.sort(Map.keys(input)) == ~w(consent operation_id revision run_id sha256 ttl),
         true <- input["consent"] == @consent,
         {:ok, owner} <- CustomerConnections.evidence_owner(actor, connection, now),
         owner = cap_expiry(owner),
         {:ok, candidate} <-
           Source.new(
             owner,
             %{run_id: input["run_id"], revision: input["revision"], digest: input["sha256"]},
             input["operation_id"],
             now,
             input["ttl"]
           ),
         :ok <- Records.active?(owner["tenant_id"], "sources", candidate["id"]),
         :ok <-
           Admission.admit(owner, "source.share", input["operation_id"], input, 1, now),
         {:ok, run} <- AgentRunEvidence.read(owner["tenant_id"], input["run_id"]),
         true <- run.revision === input["revision"] and run.digest == input["sha256"],
         {:ok, stored} <- Records.insert(owner["tenant_id"], "sources", candidate, now),
         {:ok, _} <-
           CustomerConnections.evidence_owner(actor, connection, System.system_time(:second)),
         :ok <-
           recheck(
             owner,
             Source.reference(stored),
             System.system_time(:second)
           ) do
      {:ok, %{source: Source.reference(stored), expires_at: stored["expires_at"]}}
    else
      false -> {:error, :invalid_source_request}
      {:error, :revoked} -> {:error, :source_denied}
      {:error, _} = error -> error
      _ -> {:error, :storage_unavailable}
    end
  rescue
    _ -> {:error, :storage_unavailable}
  end

  def recheck(owner, reference, now) do
    case Records.fetch(owner["tenant_id"], "sources", reference["ref"]) do
      {:ok, source} -> Source.authorize(source, owner, reference, now)
      {:error, error} when error in [:not_found, :revoked] -> {:error, :source_denied}
      error -> error
    end
  end

  def resolve(owner, reference, now) do
    with true <- is_map(reference) and Owner.id?(reference["ref"]),
         {:ok, source} <- Records.fetch(owner["tenant_id"], "sources", reference["ref"]),
         :ok <- Source.authorize(source, owner, reference, now),
         {:ok, run} <- AgentRunEvidence.read(owner["tenant_id"], source["locator"]),
         true <- run.revision == source["revision"] and run.digest == source["sha256"],
         {:ok, ^source} <- Records.fetch(owner["tenant_id"], "sources", reference["ref"]),
         :ok <- Source.authorize(source, owner, reference, System.system_time(:second)) do
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
