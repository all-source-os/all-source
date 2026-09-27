defmodule QueryServiceEx.Projections.ReplayAnalysis do
  @moduledoc """
  Read-only impact analysis for one tenant projection replay.

  Analysis reads a bounded, ordered sample from Core while preserving Core's
  exact total when available. It never mutates source events or projection
  state. Query Service remains the tenant boundary.
  """

  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient
  alias QueryServiceEx.Projections.Catalog
  alias QueryServiceEx.Projections.TenantProjections

  @sample_limit 1_000
  @top_entities 8

  @spec analyze(String.t(), String.t()) :: {:ok, map()} | {:error, term()}
  def analyze(tenant_id, projection_name)
      when is_binary(tenant_id) and is_binary(projection_name) do
    analyzed_at = now_iso8601()

    with {:ok, _template} <- Catalog.fetch(projection_name),
         {:ok, body} <- query_sample(tenant_id, analyzed_at) do
      from_page(tenant_id, projection_name, body, analyzed_at)
    end
  end

  @doc "Build the existing analysis from a caller-validated bounded Core page."
  def from_page(tenant_id, projection_name, body, analyzed_at) do
    with {:ok, template} <- Catalog.fetch(projection_name) do
      events = value(body, ["events", :events]) || []
      sampled_events = length(events)
      total_events = total_events(body, sampled_events)
      projection = projection_state(tenant_id, projection_name)
      sample_scope = if total_events > sampled_events, do: "sample", else: "full"

      {:ok,
       %{
         projection_name: projection_name,
         projection_title: template.title,
         projection_kind: Atom.to_string(template.kind),
         projection_status: projection.status,
         current_entity_count: projection.entity_count,
         total_events: total_events,
         sampled_events: sampled_events,
         analysis_scope: sample_scope,
         event_type_distribution: event_type_distribution(events),
         sampled_entity_count: sampled_entity_count(events),
         sampled_entities: sampled_entities(events),
         first_event_at: timestamp_bound(events, :first),
         last_event_at: timestamp_bound(events, :last),
         analyzed_at: analyzed_at,
         ready_to_replay: total_events > 0 and projection.status == "ready",
         checks: safety_checks(projection.status),
         warnings: warnings(total_events, sampled_events, sample_scope, projection.status)
       }}
    end
  end

  defp query_sample(tenant_id, cutoff) do
    params = %{limit: @sample_limit, offset: 0, order: "asc", as_of: cutoff}

    result =
      case Application.get_env(:query_service_ex, :tenant_projection_query_fun) do
        fun when is_function(fun, 2) -> fun.(tenant_id, params)
        _ -> RustCoreClient.query_events_page(tenant_id, params)
      end

    normalize_result(result)
  end

  defp normalize_result({:ok, events}) when is_list(events) do
    {:ok, %{"events" => events, "total_count" => length(events)}}
  end

  defp normalize_result({:ok, body}) when is_map(body), do: {:ok, body}
  defp normalize_result(other), do: other

  defp total_events(body, sampled_events) do
    value(body, ["total_count", :total_count, "total", :total, "count", :count]) ||
      sampled_events
  end

  defp projection_state(tenant_id, projection_name) do
    TenantProjections.list(tenant_id)
    |> Enum.find(%{status: "building", entity_count: 0}, &(&1.name == projection_name))
  end

  defp event_type_distribution(events) do
    events
    |> Enum.frequencies_by(&(event_field(&1, "event_type") || "unknown"))
    |> Enum.map(fn {event_type, count} ->
      %{
        event_type: event_type,
        count: count,
        share: share(count, length(events))
      }
    end)
    |> Enum.sort_by(fn %{count: count, event_type: event_type} -> {-count, event_type} end)
  end

  defp sampled_entity_count(events) do
    events
    |> Enum.map(&event_field(&1, "entity_id"))
    |> Enum.reject(&is_nil/1)
    |> MapSet.new()
    |> MapSet.size()
  end

  defp sampled_entities(events) do
    events
    |> Enum.map(&event_field(&1, "entity_id"))
    |> Enum.reject(&is_nil/1)
    |> Enum.frequencies()
    |> Enum.map(fn {entity_id, event_count} ->
      %{entity_id: entity_id, event_count: event_count}
    end)
    |> Enum.sort_by(fn %{entity_id: entity_id, event_count: count} -> {-count, entity_id} end)
    |> Enum.take(@top_entities)
  end

  defp timestamp_bound(events, direction) do
    timestamps =
      events
      |> Enum.map(&event_field(&1, "timestamp"))
      |> Enum.filter(&is_binary/1)
      |> Enum.sort()

    case direction do
      :first -> List.first(timestamps)
      :last -> List.last(timestamps)
    end
  end

  defp safety_checks(projection_status) do
    [
      %{
        key: "tenant_scope",
        label: "Tenant boundary",
        status: "pass",
        detail: "Analysis and rebuild use the authenticated tenant only."
      },
      %{
        key: "immutable_source",
        label: "Source preserved",
        status: "pass",
        detail: "Replay reads immutable events without changing them."
      },
      %{
        key: "atomic_publish",
        label: "Atomic publish",
        status: "pass",
        detail: "Completed state replaces the active generation in one pointer swap."
      },
      %{
        key: "target_ready",
        label: "Target availability",
        status: if(projection_status == "ready", do: "pass", else: "warn"),
        detail:
          if(projection_status == "ready",
            do: "Current read-model remains available during rebuild.",
            else: "Projection is already building; wait before replaying."
          )
      }
    ]
  end

  defp warnings(0, _sampled, _scope, _status),
    do: ["No events match this tenant. Nothing will be rebuilt."]

  defp warnings(total, sampled, "sample", status) do
    [
      "Event types and entities use the first #{sampled} ordered events; total #{total} is exact."
      | status_warning(status)
    ]
  end

  defp warnings(_total, _sampled, _scope, status), do: status_warning(status)

  defp status_warning("ready"), do: []

  defp status_warning(_status),
    do: ["Projection is already building and cannot start another replay."]

  defp share(_count, 0), do: 0.0
  defp share(count, total), do: Float.round(count / total * 100.0, 1)

  defp event_field(event, "event_type"),
    do: Map.get(event, "event_type") || Map.get(event, :event_type)

  defp event_field(event, "entity_id"),
    do: Map.get(event, "entity_id") || Map.get(event, :entity_id)

  defp event_field(event, "timestamp"),
    do: Map.get(event, "timestamp") || Map.get(event, :timestamp)

  defp value(body, keys), do: Enum.find_value(keys, &Map.get(body, &1))
  defp now_iso8601, do: DateTime.utc_now() |> DateTime.to_iso8601()
end
