defmodule QueryServiceExWeb.EventController do
  @moduledoc """
  Controller for event-related endpoints.

  Provides CRUD operations and querying for events.
  Uses FallbackController for consistent error handling.
  Tracks usage metering for billable operations.
  """

  use Phoenix.Controller, formats: [:json]

  plug(QueryServiceExWeb.Plugs.ManagedKeyAccess when action in [:create, :create_batch])
  use OpenApiSpex.ControllerSpecs

  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient
  alias QueryServiceExWeb.HAL
  alias QueryServiceExWeb.Schemas.Common
  alias QueryServiceExWeb.Schemas.Events

  action_fallback(QueryServiceExWeb.FallbackController)

  tags(["Events"])

  operation(:index,
    summary: "List events",
    description:
      "List events with optional filters. Results are scoped to the authenticated tenant.",
    security: [%{"bearer_auth" => []}],
    parameters: [
      entity_id: [in: :query, type: :string, description: "Filter by entity ID"],
      event_type: [in: :query, type: :string, description: "Filter by event type (exact match)"],
      event_type_prefix: [
        in: :query,
        type: :string,
        description: "Filter by event type prefix (e.g. \"prime.\" matches prime.node.created)"
      ],
      limit: [in: :query, type: :integer, description: "Maximum number of results (default: 100)"],
      offset: [in: :query, type: :integer, description: "Pagination offset (default: 0)"]
    ],
    responses: [
      ok: {"Events list", "application/json", Events.EventListResponse},
      bad_request: {"Bad request", "application/json", Common.SimpleError},
      unauthorized: {"Unauthorized", "application/json", Common.Error}
    ]
  )

  @doc """
  List events with optional filters.

  Query params:
  - entity_id: Filter by entity ID
  - event_type: Filter by event type
  - limit: Maximum number of results (default: 100)
  - offset: Pagination offset (default: 0)

  All queries are automatically filtered by the authenticated tenant.
  """
  def index(conn, params) do
    tenant_id = get_tenant_id!(conn)
    consistency = conn.assigns[:consistency]

    query_params =
      %{limit: parse_int(params["limit"], 100), offset: parse_int(params["offset"], 0)}
      |> maybe_put(:entity_id, params["entity_id"])
      |> maybe_put(:event_type, params["event_type"])
      |> maybe_put(:event_type_prefix, params["event_type_prefix"])

    case RustCoreClient.query_events(tenant_id, query_params, consistency: consistency) do
      {:ok, events} ->
        self_link =
          list_self_link(
            "/api/events",
            Map.take(params, ["entity_id", "event_type", "event_type_prefix", "limit", "offset"])
          )

        response =
          %{data: Enum.map(events, &wrap_event/1), count: length(events)}
          |> HAL.wrap(self_link)

        json(conn, response)

      {:error, reason} ->
        conn
        |> put_status(:bad_request)
        |> json(%{error: to_string(reason)})
    end
  end

  operation(:show,
    summary: "Get event by ID",
    description: "Retrieve a specific event by its ID.",
    security: [%{"bearer_auth" => []}],
    parameters: [
      id: [in: :path, type: :string, description: "Event ID", required: true]
    ],
    responses: [
      ok: {"Event details", "application/json", Events.EventResponse},
      not_found: {"Event not found", "application/json", Common.SimpleError}
    ]
  )

  # Every filter Core's GET /api/v1/events/query accepts, minus tenant_id
  # (injected from the authenticated caller) and limit/offset (parsed as ints
  # with defaults). Forwarded verbatim so a query answers the same through the
  # gateway as it does against Core directly (issue #252).
  @core_compat_filters ~w(
    entity_id event_type event_type_prefix exclude_event_type_prefix
    payload_filter since until as_of order
  )a

  @doc """
  Query events returning Core-compatible format (no HATEOAS wrapping).

  This endpoint proxies to Core's GET /api/v1/events/query and returns
  the response as-is: `{"events": [...], "count": N, "total_count": N,
  "has_more": bool}`.

  Used by SDKs that call query_events() and expect the Core wire format —
  including the pagination metadata their paginators drive off.
  """
  def query_core_compat(conn, params) do
    tenant_id = get_tenant_id!(conn)
    consistency = conn.assigns[:consistency]

    page = %{limit: parse_int(params["limit"], 100), offset: parse_int(params["offset"], 0)}

    query_params =
      Enum.reduce(@core_compat_filters, page, fn key, acc ->
        maybe_put(acc, key, params[Atom.to_string(key)])
      end)

    case RustCoreClient.query_events_page(tenant_id, query_params, consistency: consistency) do
      {:ok, %{"events" => events} = body} ->
        json(conn, core_compat_response(events, body))

      {:ok, body} when is_map(body) ->
        json(conn, body)

      {:error, reason} ->
        conn
        |> put_status(:bad_request)
        |> json(%{error: to_string(reason)})
    end
  end

  # Pagination metadata Core returns alongside the events, passed through
  # untouched. Never synthesized when absent: inventing `has_more: false` for an
  # older Core would stop a paginator early, whereas a missing key lets the
  # client fall back to its own short-page heuristic.
  @core_compat_metadata [
    {"total_count", :total_count},
    {"has_more", :has_more},
    {"entity_version", :entity_version}
  ]

  # Rebuild Core's envelope, keeping the pagination metadata (issue #252).
  defp core_compat_response(events, body) do
    base = %{events: events, count: Map.get(body, "count", length(events))}

    Enum.reduce(@core_compat_metadata, base, fn {source, key}, acc ->
      case Map.fetch(body, source) do
        {:ok, value} -> Map.put(acc, key, value)
        :error -> acc
      end
    end)
  end

  @doc """
  Get a specific event by ID.
  """
  def show(conn, %{"id" => id}) do
    case RustCoreClient.get_event_by_id(id) do
      {:ok, event} ->
        json(conn, %{data: wrap_event(event)})

      {:error, :not_found} ->
        conn
        |> put_status(:not_found)
        |> json(%{error: "Event not found"})

      {:error, reason} ->
        conn
        |> put_status(:bad_request)
        |> json(%{error: to_string(reason)})
    end
  end

  operation(:create,
    summary: "Create event",
    description:
      "Create a new event. Events are automatically associated with the authenticated tenant.",
    security: [%{"bearer_auth" => []}],
    request_body:
      {"Event to create", "application/json", Events.CreateEventRequest, required: true},
    responses: [
      created: {"Event created", "application/json", Events.EventResponse},
      unprocessable_entity: {"Validation error", "application/json", Common.SimpleError},
      unauthorized: {"Unauthorized", "application/json", Common.Error}
    ]
  )

  @doc """
  Create a new event.

  Body:
  {
    "entity_id": "string",
    "event_type": "string",
    "payload": {}
  }

  Events are automatically associated with the authenticated tenant.
  """
  def create(conn, params) do
    tenant_id = get_tenant_id!(conn)

    event =
      %{
        entity_id: params["entity_id"],
        event_type: params["event_type"],
        payload: params["payload"] || %{}
      }
      |> maybe_put_metadata(params["metadata"])

    case RustCoreClient.create_event(tenant_id, event) do
      {:ok, created_event} ->
        # Record usage after successful event creation
        record_event_usage(conn, 1, %{
          entity_id: event.entity_id,
          event_type: event.event_type
        })

        conn
        |> put_status(:created)
        |> json(%{data: wrap_event(created_event)})

      {:error, reason} ->
        conn
        |> put_status(:unprocessable_entity)
        |> json(%{error: to_string(reason)})
    end
  end

  operation(:create_batch,
    summary: "Create events batch",
    description:
      "Create multiple events in a single request. All events are associated with the authenticated tenant.",
    security: [%{"bearer_auth" => []}],
    request_body:
      {"Events to create", "application/json", Events.CreateBatchRequest, required: true},
    responses: [
      created: {"Events created", "application/json", Events.BatchCreateResponse},
      bad_request: {"Invalid request", "application/json", Common.SimpleError},
      unprocessable_entity: {"Validation error", "application/json", Common.SimpleError}
    ]
  )

  @doc """
  Create multiple events in a batch.

  Body:
  {
    "events": [
      {"entity_id": "...", "event_type": "...", "payload": {}},
      ...
    ]
  }

  All events are automatically associated with the authenticated tenant.
  """
  def create_batch(conn, %{"events" => events}) when is_list(events) do
    tenant_id = get_tenant_id!(conn)

    case RustCoreClient.create_event_batch(tenant_id, events) do
      {:ok, result} ->
        # Core returns %{"events" => [...], "ingested" => n, "total" => n}; extract
        # the list before counting/wrapping (was: length/2 on the map → 500).
        created_events = extract_created_events(result)
        event_count = length(created_events)

        record_event_usage(conn, event_count, %{
          batch: true,
          event_count: event_count
        })

        response =
          %{data: Enum.map(created_events, &wrap_event/1), count: event_count}
          |> HAL.wrap(HAL.self("/api/events/batch"))

        conn
        |> put_status(:created)
        |> json(response)

      {:error, reason} ->
        conn
        |> put_status(:unprocessable_entity)
        |> json(%{error: to_string(reason)})
    end
  end

  def create_batch(conn, _params) do
    conn
    |> put_status(:bad_request)
    |> json(%{error: "Expected 'events' array in request body"})
  end

  operation(:by_entity,
    summary: "Get events by entity",
    description: "Retrieve all events for a specific entity ID.",
    security: [%{"bearer_auth" => []}],
    parameters: [
      entity_id: [in: :path, type: :string, description: "Entity ID", required: true]
    ],
    responses: [
      ok: {"Entity events", "application/json", Events.EntityEventsResponse},
      bad_request: {"Bad request", "application/json", Common.SimpleError}
    ]
  )

  @doc """
  Get events for a specific entity.

  Results are filtered to the authenticated tenant's events only.
  """
  def by_entity(conn, %{"entity_id" => entity_id}) do
    tenant_id = get_tenant_id!(conn)
    consistency = conn.assigns[:consistency]

    case RustCoreClient.get_events_by_entity(tenant_id, entity_id, consistency: consistency) do
      {:ok, events} ->
        response =
          %{data: Enum.map(events, &wrap_event/1), count: length(events), entity_id: entity_id}
          |> HAL.wrap(HAL.self("/api/events/entity/#{entity_id}"))

        json(conn, response)

      {:error, reason} ->
        conn
        |> put_status(:bad_request)
        |> json(%{error: to_string(reason)})
    end
  end

  operation(:by_type,
    summary: "Get events by type",
    description: "Retrieve all events of a specific event type.",
    security: [%{"bearer_auth" => []}],
    parameters: [
      event_type: [in: :path, type: :string, description: "Event type", required: true]
    ],
    responses: [
      ok: {"Events by type", "application/json", Events.TypeEventsResponse},
      bad_request: {"Bad request", "application/json", Common.SimpleError}
    ]
  )

  @doc """
  Get events of a specific type.

  Results are filtered to the authenticated tenant's events only.
  """
  def by_type(conn, %{"event_type" => event_type}) do
    tenant_id = get_tenant_id!(conn)
    consistency = conn.assigns[:consistency]

    case RustCoreClient.get_events_by_type(tenant_id, event_type, consistency: consistency) do
      {:ok, events} ->
        response =
          %{data: Enum.map(events, &wrap_event/1), count: length(events), event_type: event_type}
          |> HAL.wrap(HAL.self("/api/events/type/#{event_type}"))

        json(conn, response)

      {:error, reason} ->
        conn
        |> put_status(:bad_request)
        |> json(%{error: to_string(reason)})
    end
  end

  @doc """
  Get recent events sorted by timestamp descending.

  Query params:
  - limit: Maximum number of results (default: 20)
  """
  def recent(conn, params) do
    tenant_id = get_tenant_id!(conn)
    consistency = conn.assigns[:consistency]
    limit = parse_int(params["limit"], 20)

    # Core orders by `order=asc|desc`; it has no `sort` param, so `sort` was
    # silently discarded and this endpoint served the OLDEST events (#252).
    query_params = %{
      limit: limit,
      order: "desc"
    }

    case RustCoreClient.query_events(tenant_id, query_params, consistency: consistency) do
      {:ok, events} ->
        response =
          %{data: Enum.map(events, &wrap_event/1), count: length(events)}
          |> HAL.wrap(list_self_link("/api/v1/events/recent", Map.take(params, ["limit"])))

        json(conn, response)

      {:error, reason} ->
        conn
        |> put_status(:bad_request)
        |> json(%{error: to_string(reason)})
    end
  end

  operation(:streams,
    summary: "List streams (entities)",
    description:
      "List all unique streams (entity_ids) in the event store. Streams represent unique entities that have events. Useful for discovering available entities in the system.",
    security: [%{"bearer_auth" => []}],
    parameters: [
      limit: [in: :query, type: :integer, description: "Maximum number of streams to return"],
      offset: [in: :query, type: :integer, description: "Number of streams to skip (pagination)"]
    ],
    responses: [
      ok: {"Streams list", "application/json", Events.StreamsResponse},
      bad_request: {"Bad request", "application/json", Common.SimpleError}
    ]
  )

  @doc """
  List distinct streams (entity_ids) from the event store.

  Uses dedicated Core endpoint for efficient stream discovery.
  Returns streams sorted by most recent activity first.
  """
  def streams(conn, params) do
    tenant_id = get_tenant_id!(conn)
    consistency = conn.assigns[:consistency]

    opts = [
      tenant_id: tenant_id,
      limit: parse_int(params["limit"], nil),
      offset: parse_int(params["offset"], nil),
      consistency: consistency
    ]

    self_link = list_self_link("/api/streams", Map.take(params, ["limit", "offset"]))

    case RustCoreClient.list_streams(opts) do
      {:ok, %{"streams" => streams, "total" => total}} ->
        response =
          %{data: Enum.map(streams, &wrap_stream/1), count: length(streams), total: total}
          |> HAL.wrap(self_link)

        json(conn, response)

      {:ok, response} when is_map(response) ->
        # Handle alternative response formats
        streams = Map.get(response, "streams", [])
        total = Map.get(response, "total", length(streams))

        resp =
          %{data: Enum.map(streams, &wrap_stream/1), count: length(streams), total: total}
          |> HAL.wrap(self_link)

        json(conn, resp)

      {:error, reason} ->
        conn
        |> put_status(:bad_request)
        |> json(%{error: to_string(reason)})
    end
  end

  operation(:event_types,
    summary: "List event types",
    description:
      "List all unique event types in the event store. Useful for discovering what types of events exist and their usage statistics.",
    security: [%{"bearer_auth" => []}],
    parameters: [
      limit: [in: :query, type: :integer, description: "Maximum number of event types to return"],
      offset: [
        in: :query,
        type: :integer,
        description: "Number of event types to skip (pagination)"
      ]
    ],
    responses: [
      ok: {"Event types list", "application/json", Events.EventTypesResponse},
      bad_request: {"Bad request", "application/json", Common.SimpleError}
    ]
  )

  @doc """
  List distinct event types from the event store.

  Uses dedicated Core endpoint for efficient event type discovery.
  Returns event types sorted by most used first.
  """
  def event_types(conn, params) do
    tenant_id = get_tenant_id!(conn)
    consistency = conn.assigns[:consistency]

    opts = [
      tenant_id: tenant_id,
      limit: parse_int(params["limit"], nil),
      offset: parse_int(params["offset"], nil),
      consistency: consistency
    ]

    self_link = list_self_link("/api/event-types", Map.take(params, ["limit", "offset"]))

    case RustCoreClient.list_event_types(opts) do
      {:ok, %{"event_types" => event_types, "total" => total}} ->
        response =
          %{
            data: Enum.map(event_types, &wrap_event_type/1),
            count: length(event_types),
            total: total
          }
          |> HAL.wrap(self_link)

        json(conn, response)

      {:ok, response} when is_map(response) ->
        # Handle alternative response formats
        types = Map.get(response, "event_types", [])
        total = Map.get(response, "total", length(types))

        resp =
          %{data: Enum.map(types, &wrap_event_type/1), count: length(types), total: total}
          |> HAL.wrap(self_link)

        json(conn, resp)

      {:error, reason} ->
        conn
        |> put_status(:bad_request)
        |> json(%{error: to_string(reason)})
    end
  end

  # -- HAL link helpers --

  defp event_links(event) do
    id = event["id"] || event[:id]
    entity_id = event["entity_id"] || event[:entity_id]
    event_type = event["event_type"] || event[:event_type]

    HAL.self("/api/events/#{id}")
    |> maybe_merge_link(entity_id, fn eid ->
      HAL.merge(
        HAL.link("stream", "/api/events/entity/#{eid}"),
        HAL.link("entity", "/api/events/entity/#{eid}")
      )
    end)
    |> maybe_merge_link(event_type, fn et ->
      HAL.merge(
        HAL.link("event_type", "/api/events/type/#{et}"),
        HAL.link("schema", "/api/schemas/#{et}")
      )
    end)
    |> HAL.merge(HAL.mgmt_plane_link("tenant", "/api/v1/tenants/{tenant_id}", templated: true))
  end

  defp wrap_event(event), do: HAL.wrap(event, event_links(event))

  defp maybe_merge_link(links, nil, _fun), do: links
  defp maybe_merge_link(links, value, fun), do: HAL.merge(links, fun.(value))

  # Preserve client-supplied event metadata on single create. The batch path
  # already forwards metadata verbatim; single create previously rebuilt the
  # event map with only entity_id/event_type/payload, silently dropping it.
  defp maybe_put_metadata(event, nil), do: event

  defp maybe_put_metadata(event, metadata) when is_map(metadata),
    do: Map.put(event, :metadata, metadata)

  defp maybe_put_metadata(event, _other), do: event

  # Core's batch create returns %{"events" => [...], "ingested" => n, "total" => n}.
  # Older/raw shapes may return a bare list. Normalize to the list of created events.
  defp extract_created_events(%{"events" => events}) when is_list(events), do: events
  defp extract_created_events(events) when is_list(events), do: events
  defp extract_created_events(_), do: []

  defp wrap_stream(stream) when is_binary(stream) do
    links =
      HAL.self("/api/streams")
      |> HAL.merge(HAL.link("events", "/api/events/entity/#{stream}"))

    %{"entity_id" => stream, "_links" => links}
  end

  defp wrap_stream(stream) when is_map(stream) do
    entity_id = stream["entity_id"] || stream[:entity_id]

    links =
      HAL.self("/api/streams")
      |> HAL.merge(HAL.link("events", "/api/events/entity/#{entity_id}"))

    HAL.wrap(stream, links)
  end

  defp wrap_event_type(et) when is_binary(et) do
    links =
      HAL.self("/api/event-types")
      |> HAL.merge(HAL.link("events", "/api/events/type/#{et}"))

    %{"event_type" => et, "_links" => links}
  end

  defp wrap_event_type(et) when is_map(et) do
    event_type = et["event_type"] || et[:event_type]

    links =
      HAL.self("/api/event-types")
      |> HAL.merge(HAL.link("events", "/api/events/type/#{event_type}"))

    HAL.wrap(et, links)
  end

  defp list_self_link(path, params) do
    qs = URI.encode_query(params)
    full = if qs == "", do: path, else: "#{path}?#{qs}"
    HAL.self(full)
  end

  # Helper to parse integer parameters with defaults
  defp parse_int(nil, default), do: default

  defp parse_int(value, default) when is_binary(value) do
    case Integer.parse(value) do
      {int, _} -> int
      :error -> default
    end
  end

  defp parse_int(value, _default) when is_integer(value), do: value
  defp parse_int(_, default), do: default

  # Only add param to map if value is non-nil and non-empty
  defp maybe_put(map, _key, nil), do: map
  defp maybe_put(map, _key, ""), do: map
  defp maybe_put(map, key, value), do: Map.put(map, key, value)

  # Gets tenant_id from connection, raises if not present (security check)
  defp get_tenant_id!(conn) do
    case conn.assigns[:tenant_id] do
      nil ->
        raise "Tenant context required but not present - security violation"

      tenant_id when is_binary(tenant_id) ->
        tenant_id
    end
  end

  defp record_event_usage(conn, count, _metadata) do
    case conn.assigns[:tenant_id] do
      nil -> :ok
      tenant_id -> QueryServiceEx.UsageReporter.record(tenant_id, count, :events)
    end
  end
end
