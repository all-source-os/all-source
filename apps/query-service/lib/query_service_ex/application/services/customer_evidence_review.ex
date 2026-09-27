defmodule QueryServiceEx.Application.Services.CustomerEvidenceReview do
  @moduledoc """
  Connection-owned pending comparison reviews over explicitly shared run pins.
  Calls current access checks before and after source work. No approval, external
  execution or ordinary ingestion change is added here.
  """
  alias QueryServiceEx.Application.Services.CustomerAgentAccess
  alias QueryServiceEx.Application.Services.CustomerAgentReview
  alias QueryServiceEx.Application.Services.CustomerEvidenceSources, as: Sources
  alias QueryServiceEx.Application.Services.CustomerQueryAdmission, as: Admission
  alias QueryServiceEx.Application.Services.CustomerReviewDeadline
  alias QueryServiceEx.Application.Services.CustomerReviewRecords, as: Records
  alias QueryServiceEx.Domain.AgentRun.Comparison
  alias QueryServiceEx.Domain.CustomerAgent.ConnectionConsent
  alias QueryServiceEx.Domain.CustomerAgent.PendingReview
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOperation, as: Operation
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOwner, as: Owner

  def prepare(token, binding, input, now) do
    CustomerReviewDeadline.run(binding, fn -> do_prepare(token, binding, input, now) end)
  end

  defp do_prepare(token, binding, input, now) do
    with true <-
           is_map(input) and
             Enum.sort(Map.keys(input)) == ~w(expected_revision idempotency_key proposal),
         true <-
           input["expected_revision"] === 0 and Operation.valid_at?(input["idempotency_key"], now),
         {:ok, owner} <- access(token, binding, "prepare_proposal", now),
         {:ok, proposal} <- CustomerAgentReview.validate(input["proposal"]),
         :ok <- supported(proposal),
         :ok <- recheck(owner, proposal),
         :ok <-
           Admission.admit(owner, "review.prepare", input["idempotency_key"], input, 2, now),
         {:ok, report, expiry} <- compare(owner, proposal, now),
         {:ok, record} <-
           PendingReview.new(
             owner,
             input["proposal"],
             report,
             input["idempotency_key"],
             now,
             expiry
           ),
         {:ok, stored} <- Records.insert(owner["tenant_id"], "reviews", record, now),
         :ok <- recheck(owner, proposal),
         {:ok, _} <- access(token, binding, "prepare_proposal", System.system_time(:second)),
         true <- PendingReview.digest(stored, report) == stored["digest"],
         true <- stored["expires_at"] > System.system_time(:second) do
      {:ok, receipt(stored, "pending") |> Map.put(:unknowns, report.unknowns)}
    else
      false -> {:error, :invalid_preparation}
      {:error, _} = error -> error
      _ -> {:error, :storage_unavailable}
    end
  rescue
    _ -> {:error, :storage_unavailable}
  end

  def read(token, binding, id, version, request_id, now, operation \\ "read_review") do
    CustomerReviewDeadline.run(binding, fn ->
      do_read(token, binding, id, version, request_id, now, operation)
    end)
  end

  defp do_read(token, binding, id, version, request_id, now, operation) do
    with true <- Owner.id?(id) and version === 1 and operation in ~w(read_review read_result),
         true <- Operation.valid_at?(request_id, now),
         {:ok, owner} <- access(token, binding, operation, now),
         {:ok, record} <- Records.fetch(owner["tenant_id"], "reviews", id),
         true <- Owner.matches?(record, owner) and now >= record["created_at"],
         {:ok, response} <- current_view(owner, record, operation, request_id, now),
         {:ok, ^record} <- Records.fetch(owner["tenant_id"], "reviews", id),
         {:ok, _} <- access(token, binding, operation, System.system_time(:second)) do
      if record["expires_at"] <= System.system_time(:second),
        do: {:ok, receipt(record, "expired")},
        else: {:ok, response}
    else
      {:error, error} when error in [:not_found, :revoked] -> {:error, :access_denied}
      false -> {:error, :access_denied}
      {:error, _} = error -> error
      _ -> {:error, :access_denied}
    end
  rescue
    _ -> {:error, :storage_unavailable}
  end

  defp current_view(owner, record, operation, request_id, now) do
    if now >= record["expires_at"] do
      {:ok, receipt(record, "expired")}
    else
      with {:ok, proposal} <- CustomerAgentReview.validate(record["proposal"]),
           :ok <- supported(proposal),
           :ok <- recheck(owner, proposal),
           purpose = if(operation == "read_result", do: "review.result", else: "review.read"),
           :ok <-
             Admission.admit(owner, purpose, request_id, [record["id"], record["digest"]], 2, now),
           {:ok, report, _expiry} <- compare(owner, proposal, now),
           :ok <- recheck(owner, proposal),
           true <- PendingReview.digest(record, report) == record["digest"] do
        if operation == "read_result" do
          {:ok, receipt(record, "pending") |> Map.put(:result_available, false)}
        else
          {:ok,
           receipt(record, "pending")
           |> Map.merge(%{
             proposal: record["proposal"],
             evidence: report,
             unknowns: report.unknowns
           })}
        end
      else
        false -> {:ok, receipt(record, "superseded")}
        {:error, :source_changed} -> {:ok, receipt(record, "superseded")}
        {:error, :source_expired} -> {:ok, receipt(record, "expired")}
        {:error, :source_denied} -> {:ok, receipt(record, "unavailable")}
        {:error, :revoked} -> {:ok, receipt(record, "unavailable")}
        {:error, _} = error -> error
        _ -> {:error, :storage_unavailable}
      end
    end
  end

  defp supported(%{
         kind: "run_comparison",
         sources: [%{"kind" => "run_evidence"}, %{"kind" => "run_evidence"}]
       }),
       do: :ok

  defp supported(_), do: {:error, :unsupported_evidence}

  defp compare(owner, %{kind: "run_comparison", sources: [first, second]}, now) do
    with true <- first["kind"] == "run_evidence" and second["kind"] == "run_evidence",
         {:ok, baseline, first_expiry} <- Sources.resolve(owner, first, now),
         {:ok, candidate, second_expiry} <- Sources.resolve(owner, second, now),
         report = Comparison.compare(baseline, candidate),
         true <- byte_size(Jason.encode!(report)) <= 48_000 do
      deadlines = [first_expiry, second_expiry, owner["grant_expires_at"], now + 86_400]

      deadlines =
        if owner["entitlement_expires_at"],
          do: [owner["entitlement_expires_at"] | deadlines],
          else: deadlines

      {:ok, report, Enum.min(deadlines)}
    else
      false -> {:error, :unsupported_evidence}
      {:error, _} = error -> error
    end
  end

  defp compare(_, _, _), do: {:error, :unsupported_evidence}

  defp recheck(owner, proposal) do
    Enum.reduce_while(proposal.sources, :ok, fn reference, :ok ->
      case Sources.recheck(owner, reference, System.system_time(:second)) do
        :ok -> {:cont, :ok}
        error -> {:halt, error}
      end
    end)
  end

  defp access(token, binding, operation, now) do
    with {:ok, owner} <- CustomerAgentAccess.verify_metered(token, binding, operation, now),
         true <- owner["consent_version"] == ConnectionConsent.evidence_version() do
      {:ok, owner}
    else
      false -> {:error, :access_denied}
      error -> error
    end
  end

  defp receipt(record, state),
    do: %{
      schema_version: 1,
      id: record["id"],
      version: record["version"],
      digest: record["digest"],
      state: state,
      expires_at: record["expires_at"],
      view_schema: record["view_schema"],
      persisted: true,
      approved: false,
      execution: "none",
      human_approval: "required_in_product"
    }
end
