defmodule QueryServiceEx.Domain.CustomerAgent.ReplaySource do
  @moduledoc "Owner-bound replay-analysis facts, not an approval or a frozen tenant event set."
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOperation, as: Operation
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOwner, as: Owner
  alias QueryServiceEx.Projections.Catalog

  @keys ~w(schema_version id tenant_id subject_id client_id resource grant_id kind locator revision sha256 request_sha256 created_at expires_at snapshot)

  def new(owner, snapshot, operation, now, ttl) do
    with true <- Owner.valid?(owner) and Operation.valid_at?(operation, now),
         true <- valid_snapshot?(snapshot),
         true <- is_integer(ttl) and ttl in 1..3_600,
         expiry =
           Enum.min([
             now + ttl,
             owner["grant_expires_at"],
             owner["entitlement_expires_at"] || now + ttl
           ]),
         true <- Owner.timestamp?(expiry) and expiry > now do
      {:ok,
       Owner.take(owner)
       |> Map.merge(%{
         "schema_version" => 1,
         "id" => Owner.object_id(owner, "source", operation),
         "kind" => "replay_analysis",
         "locator" => snapshot["analysis"]["projection_name"],
         "revision" => 1,
         "sha256" => Owner.digest(snapshot),
         "request_sha256" => Owner.digest([snapshot, ttl]),
         "created_at" => now,
         "expires_at" => expiry,
         "snapshot" => snapshot
       })}
    else
      _ -> {:error, :invalid_source}
    end
  end

  def valid?(value, tenant) when is_map(value) do
    Enum.sort(Map.keys(value)) == Enum.sort(@keys) and
      value["schema_version"] === 1 and value["kind"] == "replay_analysis" and
      value["revision"] === 1 and Owner.valid?(value) and value["tenant_id"] == tenant and
      Owner.id?(value["id"]) and valid_snapshot?(value["snapshot"]) and
      valid_content?(value) and valid_interval?(value)
  end

  def valid?(_, _), do: false

  defp valid_content?(value),
    do:
      value["locator"] == value["snapshot"]["analysis"]["projection_name"] and
        value["sha256"] == Owner.digest(value["snapshot"]) and
        Owner.hash?(value["request_sha256"])

  defp valid_interval?(value),
    do:
      Owner.timestamp?(value["created_at"]) and Owner.timestamp?(value["expires_at"]) and
        (value["expires_at"] - value["created_at"]) in 1..3_600

  def valid_snapshot?(
        %{"analysis" => analysis, "sample_sha256" => sample, "catalog_sha256" => catalog} = value
      )
      when map_size(value) == 3 and is_map(analysis) do
    Enum.sort(Map.keys(analysis)) ==
      ~w(analysis_scope analyzed_at current_entity_count projection_name projection_status reported_total_events sampled_entity_count sampled_events unknowns) and
      Catalog.valid?(analysis["projection_name"]) and Owner.hash?(sample) and Owner.hash?(catalog) and
      valid_counts?(analysis) and valid_scope?(analysis) and timestamp?(analysis["analyzed_at"]) and
      analysis["unknowns"] == unknowns()
  end

  def valid_snapshot?(_), do: false

  defp valid_counts?(analysis) do
    Enum.all?(
      ~w(current_entity_count reported_total_events sampled_entity_count sampled_events),
      fn key ->
        is_nil(analysis[key]) or
          (is_integer(analysis[key]) and analysis[key] in 0..9_007_199_254_740_991)
      end
    ) and analysis["sampled_events"] in 0..1_000 and
      analysis["sampled_entity_count"] in 0..1_000 and
      analysis["sampled_entity_count"] <= analysis["sampled_events"] and
      (is_nil(analysis["reported_total_events"]) or
         analysis["reported_total_events"] >= analysis["sampled_events"])
  end

  defp valid_scope?(analysis) do
    analysis["projection_status"] in [nil, "ready", "building"] and
      case analysis["analysis_scope"] do
        nil ->
          true

        "full" ->
          analysis["reported_total_events"] == analysis["sampled_events"]

        "sample" ->
          is_integer(analysis["reported_total_events"]) and
            analysis["reported_total_events"] > analysis["sampled_events"]

        _ ->
          false
      end
  end

  def unknowns,
    do:
      ~w(total_count_provenance authoritative_order restart_proof run_comparison archive_completeness)

  def catalog_digest, do: Owner.digest(Base.encode16(Catalog.module_info(:md5), case: :lower))

  defp timestamp?(value) when is_binary(value) and byte_size(value) <= 40,
    do: match?({:ok, _, 0}, DateTime.from_iso8601(value))

  defp timestamp?(_), do: false
end
