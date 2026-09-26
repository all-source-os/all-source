defmodule QueryServiceEx.Application.Services.CustomerAgentReview do
  @moduledoc """
  Customer review contract bindings to the existing projection domain.

  The arbitrary-query and replay controllers cannot be reused as a host disclosure
  boundary. This service validates catalog targets and selects safe fields from
  ReplayAnalysis results. It performs no fetch, persistence, approval or rebuild.
  """

  alias QueryServiceEx.Domain.CustomerAgent.Proposal
  alias QueryServiceEx.Projections.Catalog

  @max_integer 9_007_199_254_740_991
  @count_fields [:total_events, :sampled_events, :current_entity_count, :sampled_entity_count]

  @doc "Validate proposal syntax and the actual curated projection target."
  @spec validate(term()) :: {:ok, Proposal.t()} | {:error, atom()}
  def validate(input) do
    with {:ok, proposal} <- Proposal.validate(input),
         :ok <- target(proposal) do
      {:ok, proposal}
    end
  end

  @doc "Whitelist bounded metadata from a server-owned ReplayAnalysis result."
  @spec replay_snapshot(term()) :: {:ok, map()} | {:error, atom()}
  def replay_snapshot(%{projection_name: name} = analysis) do
    with :ok <- projection(name),
         true <- valid_analysis?(analysis) do
      {:ok,
       %{
         "projection_name" => name,
         "projection_status" => analysis[:projection_status],
         "reported_total_events" => analysis[:total_events],
         "sampled_events" => analysis[:sampled_events],
         "current_entity_count" => analysis[:current_entity_count],
         "sampled_entity_count" => analysis[:sampled_entity_count],
         "analysis_scope" => analysis[:analysis_scope],
         "analyzed_at" => analysis[:analyzed_at],
         "unknowns" => [
           "total_count_provenance",
           "authoritative_order",
           "restart_proof",
           "run_comparison"
         ]
       }}
    else
      false -> {:error, :invalid_analysis}
      error -> error
    end
  end

  def replay_snapshot(_), do: {:error, :invalid_analysis}

  defp target(%Proposal{kind: "replay_plan", projection_name: name}), do: projection(name)
  defp target(_), do: :ok

  defp projection(name) do
    if Catalog.valid?(name), do: :ok, else: {:error, :unknown_projection}
  end

  defp valid_analysis?(analysis) do
    valid_counts?(analysis) and valid_scope?(analysis) and
      valid_timestamp?(analysis[:analyzed_at])
  end

  defp valid_counts?(analysis) do
    Enum.all?(@count_fields, &valid_count?(analysis[&1])) and
      at_most?(analysis[:sampled_events], 1_000) and
      at_most?(analysis[:sampled_entity_count], 1_000) and
      at_most?(analysis[:sampled_entity_count], analysis[:sampled_events]) and
      at_most?(analysis[:sampled_events], analysis[:total_events])
  end

  defp valid_scope?(analysis) do
    analysis[:projection_status] in [nil, "ready", "building"] and
      analysis[:analysis_scope] in [nil, "sample", "full"] and consistent_scope?(analysis)
  end

  defp consistent_scope?(%{analysis_scope: scope, total_events: total, sampled_events: sampled})
       when is_integer(total) and is_integer(sampled) do
    case scope do
      "full" -> total == sampled
      "sample" -> total > sampled
      nil -> true
    end
  end

  defp consistent_scope?(_), do: true

  defp valid_count?(nil), do: true
  defp valid_count?(value), do: is_integer(value) and value in 0..@max_integer
  defp at_most?(nil, _), do: true
  defp at_most?(_, nil), do: true
  defp at_most?(value, limit), do: value <= limit
  defp valid_timestamp?(nil), do: true

  defp valid_timestamp?(value) when is_binary(value) and byte_size(value) <= 40,
    do: match?({:ok, _, _}, DateTime.from_iso8601(value))

  defp valid_timestamp?(_), do: false
end
