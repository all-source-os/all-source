defmodule QueryServiceEx.Application.Services.AgentRunEvidence do
  @moduledoc """
  Internal read-only run evidence. The caller must establish current tenant and
  source disclosure authority; this service is not an authentication boundary.
  No public route or MCP tool exposes it until that boundary is connected.
  """
  alias QueryServiceEx.Domain.AgentRun.Comparison
  alias QueryServiceEx.Domain.AgentRun.Timeline

  def read(tenant, run_id) do
    source = Application.fetch_env!(:query_service_ex, :agent_run_source)

    with {:ok, events} <- source.events(tenant, run_id) do
      Timeline.build(tenant, run_id, events)
    end
  end

  def compare(tenant, baseline_id, candidate_id) do
    with {:ok, baseline} <- read(tenant, baseline_id),
         {:ok, candidate} <- read(tenant, candidate_id) do
      {:ok, Comparison.compare(baseline, candidate)}
    end
  end

  @doc "Page a validated immutable revision; changed history requires a fresh first page."
  def page(run, expected_digest, after_version, limit)
      when is_integer(after_version) and after_version >= 0 and is_integer(limit) and
             limit in 1..100 do
    if expected_digest == run.digest and after_version <= run.revision do
      events = Enum.slice(run.events, after_version, limit)
      next = after_version + length(events)

      {:ok,
       %{
         events: events,
         revision: run.revision,
         digest: run.digest,
         next_version: if(next < run.revision, do: next, else: nil)
       }}
    else
      {:error, :stale_revision}
    end
  end

  def page(_, _, _, _), do: {:error, :invalid_page}
end
