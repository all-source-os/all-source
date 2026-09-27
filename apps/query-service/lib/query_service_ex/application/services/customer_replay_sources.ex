defmodule QueryServiceEx.Application.Services.CustomerReplaySources do
  @moduledoc "Explicit, metered selection and revalidation of bounded replay-analysis facts."
  alias QueryServiceEx.Application.Services.CustomerAgentReview
  alias QueryServiceEx.Application.Services.CustomerConnections, as: Connections
  alias QueryServiceEx.Application.Services.CustomerQueryAdmission, as: Admission
  alias QueryServiceEx.Application.Services.CustomerReviewDeadline
  alias QueryServiceEx.Application.Services.CustomerReviewRecords, as: Records
  alias QueryServiceEx.Domain.CustomerAgent.ConnectionConsent
  alias QueryServiceEx.Domain.CustomerAgent.EvidenceSource
  alias QueryServiceEx.Domain.CustomerAgent.ReplaySource
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOperation, as: Operation
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOwner, as: Owner
  alias QueryServiceEx.Infrastructure.Adapters.ReplayAnalysisStore
  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient
  alias QueryServiceEx.Projections.Catalog
  alias QueryServiceEx.Projections.ReplayAnalysis

  def owner(actor, connection, now, operation \\ "prepare_proposal") do
    with {:ok, owner} <- Connections.evidence_owner(actor, connection, now, operation),
         true <- owner["consent_version"] == ConnectionConsent.replay_version() do
      {:ok, owner}
    else
      false -> {:error, :access_denied}
      error -> error
    end
  end

  def inspect_source(actor, connection, input, now) do
    CustomerReviewDeadline.run(actor, fn ->
      with true <- is_map(input) and Enum.sort(Map.keys(input)) == ~w(projection_name request_id),
           true <- Operation.valid_at?(input["request_id"], now),
           {:ok, owner} <- owner(actor, connection, now),
           :ok <- enabled(owner, input["projection_name"]),
           :ok <- Admission.admit(owner, "replay.inspect", input["request_id"], input, 1, now),
           {:ok, snapshot} <- snapshot(owner["tenant_id"], input["projection_name"]),
           {:ok, _} <- owner(actor, connection, System.system_time(:second)) do
        {:ok, %{snapshot: snapshot, sha256: Owner.digest(snapshot), shared: false}}
      else
        false -> {:error, :invalid_source_request}
        error -> error
      end
    end)
  end

  def share(actor, connection, input, now) do
    CustomerReviewDeadline.run(actor, fn ->
      with true <-
             is_map(input) and Enum.sort(Map.keys(input)) == ~w(consent operation_id snapshot ttl),
           true <-
             input["consent"] == %{"accepted" => true, "version" => "selected-replay-analysis-v1"},
           true <- ReplaySource.valid_snapshot?(input["snapshot"]),
           {:ok, owner} <- owner(actor, connection, now),
           :ok <- enabled(owner, input["snapshot"]["analysis"]["projection_name"]),
           :ok <- Admission.admit(owner, "replay.share", input["operation_id"], input, 1, now),
           {:ok, current} <-
             snapshot(owner["tenant_id"], input["snapshot"]["analysis"]["projection_name"]),
           :ok <- same_facts(input["snapshot"], current),
           {:ok, candidate} <-
             ReplaySource.new(owner, current, input["operation_id"], now, input["ttl"]),
           candidate =
             Map.put(candidate, "request_sha256", Owner.digest([input["snapshot"], input["ttl"]])),
           {:ok, stored} <- Records.insert(owner["tenant_id"], "sources", candidate, now),
           {:ok, _} <- owner(actor, connection, System.system_time(:second)) do
        {:ok, %{source: EvidenceSource.reference(stored), expires_at: stored["expires_at"]}}
      else
        false -> {:error, :invalid_source_request}
        error -> error
      end
    end)
  end

  def resolve(owner, reference, now) do
    with true <- reference["kind"] == "replay_analysis",
         {:ok, source} <- Records.fetch(owner["tenant_id"], "sources", reference["ref"]),
         :ok <- EvidenceSource.authorize(source, owner, reference, now),
         :ok <- enabled(owner, source["locator"]),
         :ok <- fresh(owner["tenant_id"], source),
         {:ok, ^source} <- Records.fetch(owner["tenant_id"], "sources", reference["ref"]),
         :ok <- EvidenceSource.authorize(source, owner, reference, System.system_time(:second)) do
      {:ok, source}
    else
      false -> {:error, :source_denied}
      {:error, _} = error -> error
      _ -> {:error, :source_changed}
    end
  end

  def enabled(owner, projection) do
    with {:ok, _} <- Catalog.fetch(projection),
         {:ok, tenant} <- RustCoreClient.get_tenant_for_authorization(owner["tenant_id"]),
         names when is_list(names) <- get_in(tenant, ["metadata", "projections", "enabled"]),
         true <- projection in names do
      :ok
    else
      {:error, _} = error -> error
      _ -> {:error, :projection_not_enabled}
    end
  end

  defp fresh(tenant, source) do
    with {:ok, current} <- snapshot(tenant, source["locator"]) do
      # The source retains its original analysis time. Freshness compares facts,
      # sample bytes and reducer revision, not the act of checking the clock.
      same_facts(source["snapshot"], current)
    end
  end

  defp same_facts(pinned, current) do
    normalize = fn value -> update_in(value, ["analysis"], &Map.delete(&1, "analyzed_at")) end
    if normalize.(pinned) == normalize.(current), do: :ok, else: {:error, :source_changed}
  end

  defp snapshot(tenant, projection) do
    at = DateTime.utc_now() |> DateTime.to_iso8601()

    with {:ok, page} <- ReplayAnalysisStore.sample(tenant, at),
         {:ok, analysis} <- ReplayAnalysis.from_page(tenant, projection, page, at),
         {:ok, safe} <- CustomerAgentReview.replay_snapshot(analysis) do
      safe =
        safe
        |> Map.put("unknowns", ReplaySource.unknowns())
        |> Map.put("reported_total_events", page["total_count"])

      safe = if is_nil(page["total_count"]), do: Map.put(safe, "analysis_scope", nil), else: safe

      value = %{
        "analysis" => safe,
        "sample_sha256" => Owner.digest(page["events"]),
        "catalog_sha256" => ReplaySource.catalog_digest()
      }

      if ReplaySource.valid_snapshot?(value),
        do: {:ok, value},
        else: {:error, :source_unavailable}
    end
  end
end
