defmodule QueryServiceEx.Infrastructure.Adapters.RustCoreClient do
  @moduledoc """
  HTTP client adapter for communicating with the Rust Event Store Core.

  Implements the QueryExecutor protocol and provides methods for:
  - Event ingestion
  - Event querying
  - Schema management
  - Snapshot operations

  Uses Tesla for HTTP client with connection pooling via Hackney.

  ## Authentication

  The client authenticates to Core using the `CORE_API_KEY` environment variable.
  This key is sent as an Authorization header with all requests.

  ## Tenant Isolation

  All data operations require a `tenant_id` parameter to ensure strict
  tenant isolation. Events are stored with tenant_id and all queries
  are automatically filtered by tenant_id.
  """

  alias QueryServiceEx.Domain.Entities.Query
  alias QueryServiceEx.Infrastructure.Adapters.CoreHealthChecker

  require Logger

  @default_base_url "http://localhost:3900"
  @default_timeout 30_000
  @read_counter_name :core_read_round_robin

  @doc false
  def client do
    write_client()
  end

  @doc """
  Returns a Tesla client targeting the write (leader) URL.

  Uses `core_write_url` config, falling back to `core_url`.
  All POST/PUT/DELETE calls should use this client.
  """
  def write_client do
    base_url =
      Application.get_env(:query_service_ex, :core_write_url) ||
        Application.get_env(:query_service_ex, :core_url, @default_base_url)

    build_client(base_url)
  end

  @doc """
  Returns a Tesla client respecting the given consistency mode.

  - `:strong` — routes to the leader (write URL) for read-your-writes
  - `:eventual` (default) — round-robin across healthy followers
  """
  def client_for_consistency(:strong), do: write_client()
  def client_for_consistency(_), do: read_client()

  @doc """
  Returns a Tesla client targeting one of the healthy read URLs via round-robin.

  Checks CoreHealthChecker ETS state to skip unhealthy or lagging nodes.
  If all read nodes are unhealthy, falls back to the write (leader) URL.
  Uses `core_read_urls` config (list), falling back to `core_url`.
  All GET calls should use this client.
  """
  def read_client do
    all_urls =
      case Application.get_env(:query_service_ex, :core_read_urls) do
        urls when is_list(urls) and urls != [] -> urls
        _ -> [Application.get_env(:query_service_ex, :core_url, @default_base_url)]
      end

    healthy_urls = CoreHealthChecker.healthy_read_urls()

    # Use healthy subset if any are available, otherwise fall back to leader
    urls =
      case Enum.filter(all_urls, &(&1 in healthy_urls)) do
        [] ->
          # All read nodes unhealthy — fall back to write (leader) URL
          write_url =
            Application.get_env(:query_service_ex, :core_write_url) ||
              Application.get_env(:query_service_ex, :core_url, @default_base_url)

          [write_url]

        available ->
          available
      end

    base_url = pick_read_url(urls)
    build_client(base_url)
  end

  defp build_client(base_url, opts \\ []) do
    middleware = [
      {Tesla.Middleware.BaseUrl, base_url},
      Tesla.Middleware.JSON,
      {Tesla.Middleware.Timeout, timeout: Keyword.get(opts, :timeout, @default_timeout)},
      {Tesla.Middleware.Retry,
       delay: 100,
       max_retries: Keyword.get(opts, :max_retries, 3),
       max_delay: 2_000,
       should_retry: fn
         {:ok, %{status: status}} when status in [408, 429, 500, 502, 503, 504] -> true
         {:ok, _} -> false
         {:error, _} -> true
       end}
    ]

    # Add Authorization header if CORE_API_KEY is configured
    middleware =
      case Application.get_env(:query_service_ex, :core_api_key) do
        nil -> middleware
        "" -> middleware
        api_key -> [{Tesla.Middleware.Headers, [{"authorization", api_key}]} | middleware]
      end

    Tesla.client(middleware, {Tesla.Adapter.Hackney, [connect_options: [:inet6]]})
  end

  defp pick_read_url([single_url]), do: single_url

  defp pick_read_url(urls) do
    count = length(urls)
    ref = ensure_counter()
    index = :atomics.add_get(ref, 1, 1)
    Enum.at(urls, rem(abs(index), count))
  end

  defp ensure_counter do
    case :persistent_term.get(@read_counter_name, nil) do
      nil ->
        ref = :atomics.new(1, signed: true)
        :persistent_term.put(@read_counter_name, ref)
        ref

      ref ->
        ref
    end
  end

  ## Event Management

  @doc """
  Create a single event with tenant isolation.

  ## Parameters
    * `tenant_id` - The tenant ID (required for isolation)
    * `event` - Map with event data

  ## Returns
    * `{:ok, event}` - Created event
    * `{:error, reason}` - Error details

  ## Examples

      iex> create_event("tenant-uuid", %{
      ...>   entity_id: "user-123",
      ...>   event_type: "user.created",
      ...>   payload: %{email: "user@example.com"}
      ...> })
      {:ok, %{id: "evt-123", ...}}
  """
  def create_event(tenant_id, event) when is_binary(tenant_id) and is_map(event) do
    event_with_tenant = Map.put(event, :tenant_id, tenant_id)

    case Tesla.post(write_client(), "/api/v1/events", event_with_tenant) do
      {:ok, %Tesla.Env{status: status, body: body}} when status in [200, 201] ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  # Deprecated: Use create_event/2 with tenant_id for proper isolation
  @doc false
  def create_event(event) when is_map(event) do
    case Tesla.post(write_client(), "/api/v1/events", event) do
      {:ok, %Tesla.Env{status: status, body: body}} when status in [200, 201] ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  Create multiple events in a batch with tenant isolation.

  Uses the Core batch endpoint for efficient bulk ingestion.

  ## Parameters
    * `tenant_id` - The tenant ID (required for isolation)
    * `events` - List of event maps

  ## Returns
    * `{:ok, response}` - Batch response with `total`, `ingested`, and `events` list
    * `{:error, reason}` - Error details
  """
  def create_event_batch(tenant_id, events) when is_binary(tenant_id) and is_list(events) do
    events_with_tenant = Enum.map(events, &Map.put(&1, :tenant_id, tenant_id))

    case Tesla.post(write_client(), "/api/v1/events/batch", %{events: events_with_tenant}) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: 201, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  # Deprecated: Use create_event_batch/2 with tenant_id for proper isolation
  @doc false
  def create_event_batch(events) when is_list(events) do
    case Tesla.post(write_client(), "/api/v1/events/batch", %{events: events}) do
      {:ok, %Tesla.Env{status: 201, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  Query events from the event store with tenant isolation.

  ## Parameters
    * `tenant_id` - The tenant ID (required for isolation)
    * `query` - Query struct or map with query parameters

  ## Returns
    * `{:ok, events}` - List of matching events
    * `{:error, reason}` - Error details

  ## Examples

      iex> query_events("tenant-uuid", %{entity_id: "user-123", limit: 10})
      {:ok, [%{id: "evt-1", ...}, ...]}
  """
  def query_events(tenant_id, query, opts \\ [])

  def query_events(tenant_id, %Query{} = query, opts) when is_binary(tenant_id) do
    params = compile_query(query)
    query_events(tenant_id, params, opts)
  end

  def query_events(tenant_id, params, opts) when is_binary(tenant_id) and is_map(params) do
    params_with_tenant = enforce_tenant(params, tenant_id)
    client = client_for_consistency(Keyword.get(opts, :consistency))

    case Tesla.get(client, "/api/v1/events/query", query: params_with_tenant) do
      {:ok, %Tesla.Env{status: 200, body: %{"events" => events}}} when is_list(events) ->
        {:ok, events}

      {:ok, %Tesla.Env{status: 200, body: body}} when is_list(body) ->
        {:ok, body}

      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  Query events and return Core's full response envelope with tenant isolation.

  Unlike `query_events/3`, which unwraps to just the event list, this keeps the
  pagination metadata Core returns (`total_count`, `has_more`, `entity_version`).
  Callers that proxy Core's wire format to clients must use this — dropping
  `has_more` forces SDK paginators onto the `fetched < page_size` heuristic,
  which is only correct when the last page is short (issue #252).

  ## Returns
    * `{:ok, %{"events" => [...], "count" => N, ...}}` - Core's response body
    * `{:error, reason}` - Error details
  """
  def query_events_page(tenant_id, params, opts \\ [])
      when is_binary(tenant_id) and is_map(params) do
    params_with_tenant = enforce_tenant(params, tenant_id)
    client = client_for_consistency(Keyword.get(opts, :consistency))

    case Tesla.get(client, "/api/v1/events/query", query: params_with_tenant) do
      {:ok, %Tesla.Env{status: 200, body: %{"events" => events} = body}} when is_list(events) ->
        {:ok, body}

      {:ok, %Tesla.Env{status: 200, body: body}} when is_list(body) ->
        {:ok, %{"events" => body, "count" => length(body)}}

      {:ok, %Tesla.Env{status: 200, body: body}} when is_map(body) ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  # Stamp the authenticated tenant onto the outbound Core query, AUTHORITATIVELY.
  #
  # `Map.put(params, :tenant_id, ...)` only overrides the atom key. Callers that
  # forward a raw request map (QueryController.build_simple_query/1 hands the
  # request body straight through) carry a STRING `"tenant_id"` key, so the map
  # ended up holding both and Tesla encoded both:
  #
  #     tenant_id=authenticated-tenant&limit=5&tenant_id=victim-tenant
  #
  # Core's axum extractor rejects that duplicate today (serde_urlencoded errors
  # with `duplicate field \`tenant_id\``), so a legitimate query carrying a
  # tenant_id in its body 400s — and the isolation only holds because of a
  # parser detail on the other side of the network. Delete every spelling first
  # so exactly one tenant_id, ours, goes on the wire.
  defp enforce_tenant(params, tenant_id) do
    params
    |> Map.delete("tenant_id")
    |> Map.delete(:tenant_id)
    |> Map.put(:tenant_id, tenant_id)
  end

  # Deprecated: Use query_events/2 with tenant_id for proper isolation
  @doc false
  def query_events(%Query{} = query) do
    params = compile_query(query)
    query_events(params)
  end

  @doc false
  def query_events(params) when is_map(params) do
    case Tesla.get(read_client(), "/api/v1/events/query", query: params) do
      {:ok, %Tesla.Env{status: 200, body: %{"events" => events}}} when is_list(events) ->
        {:ok, events}

      {:ok, %Tesla.Env{status: 200, body: body}} when is_list(body) ->
        {:ok, body}

      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Get events by entity ID with tenant isolation"
  def get_events_by_entity(tenant_id, entity_id, opts \\ []) when is_binary(tenant_id) do
    query_events(tenant_id, %{entity_id: entity_id}, opts)
  end

  # Deprecated: Use get_events_by_entity/2 with tenant_id
  @doc false
  def get_events_by_entity(entity_id) do
    query_events(%{entity_id: entity_id})
  end

  @doc "Get events by event type with tenant isolation"
  def get_events_by_type(tenant_id, event_type, opts \\ []) when is_binary(tenant_id) do
    query_events(tenant_id, %{event_type: event_type}, opts)
  end

  # Deprecated: Use get_events_by_type/2 with tenant_id
  @doc false
  def get_events_by_type(event_type) do
    query_events(%{event_type: event_type})
  end

  ## Streams (Entity Discovery)

  @doc """
  List all streams (entity_ids) in the event store.

  Streams represent unique entity_ids that have events. This endpoint
  is useful for discovering available entities in the system.

  ## Parameters
    * `opts` - Keyword list of options:
      * `:limit` - Maximum number of streams to return
      * `:offset` - Number of streams to skip (for pagination)

  ## Returns
    * `{:ok, %{streams: [...], total: N}}` - List of streams with metadata
    * `{:error, reason}` - Error details

  ## Examples

      iex> list_streams(limit: 10)
      {:ok, %{streams: [%{stream_id: "user-123", event_count: 42, ...}], total: 150}}
  """
  def list_streams(opts \\ []) do
    {consistency, opts} = Keyword.pop(opts, :consistency)

    query_params =
      opts
      |> Keyword.take([:tenant_id, :limit, :offset])
      |> Enum.reject(fn {_k, v} -> is_nil(v) end)
      |> Map.new()

    client = client_for_consistency(consistency)

    case Tesla.get(client, "/api/v1/streams", query: query_params) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  ## Event Types (Type Discovery)

  @doc """
  List all unique event types in the event store.

  This endpoint is useful for discovering what types of events
  exist in the system and their usage statistics.

  ## Parameters
    * `opts` - Keyword list of options:
      * `:limit` - Maximum number of event types to return
      * `:offset` - Number of event types to skip (for pagination)

  ## Returns
    * `{:ok, %{event_types: [...], total: N}}` - List of event types with metadata
    * `{:error, reason}` - Error details

  ## Examples

      iex> list_event_types(limit: 10)
      {:ok, %{event_types: [%{event_type: "user.created", event_count: 1500, ...}], total: 25}}
  """
  def list_event_types(opts \\ []) do
    {consistency, opts} = Keyword.pop(opts, :consistency)

    query_params =
      opts
      |> Keyword.take([:tenant_id, :limit, :offset])
      |> Enum.reject(fn {_k, v} -> is_nil(v) end)
      |> Map.new()

    client = client_for_consistency(consistency)

    case Tesla.get(client, "/api/v1/event-types", query: query_params) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  ## Projections

  @doc "List all projections"
  def list_projections do
    case Tesla.get(read_client(), "/api/v1/projections") do
      {:ok, %Tesla.Env{status: 200, body: %{"projections" => projections}}}
      when is_list(projections) ->
        {:ok, projections}

      {:ok, %Tesla.Env{status: 200, body: body}} when is_list(body) ->
        {:ok, body}

      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Get a specific projection by ID"
  def get_projection(id) do
    case Tesla.get(read_client(), "/api/v1/projections/#{id}") do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: 404}} ->
        {:error, :not_found}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Get a single event by ID"
  def get_event_by_id(event_id) do
    case Tesla.get(read_client(), "/api/v1/events/#{event_id}") do
      {:ok, %Tesla.Env{status: 200, body: %{"event" => event, "found" => true}}} ->
        {:ok, event}

      {:ok, %Tesla.Env{status: 200, body: %{"found" => false}}} ->
        {:error, :not_found}

      {:ok, %Tesla.Env{status: 404}} ->
        {:error, :not_found}

      {:ok, %Tesla.Env{status: status, body: body}} when is_map(body) or is_list(body) ->
        {:error, "HTTP #{status}: #{Jason.encode!(body)}"}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{body}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Delete (clear) a projection by name"
  def delete_projection(name) do
    case Tesla.delete(write_client(), "/api/v1/projections/#{name}") do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: 404}} ->
        {:error, :not_found}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Get aggregate projection state (all entities)"
  def get_projection_state_summary(name) do
    case Tesla.get(read_client(), "/api/v1/projections/#{name}/state") do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: 404}} ->
        {:error, :not_found}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Reset a projection to initial state and reprocess events"
  def reset_projection(name) do
    case Tesla.post(write_client(), "/api/v1/projections/#{name}/reset", %{}) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: 404}} ->
        {:error, :not_found}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Pause a projection"
  def pause_projection(name) do
    case Tesla.post(write_client(), "/api/v1/projections/#{name}/pause", %{}) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: 404}} ->
        {:error, :not_found}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Start (resume) a projection"
  def start_projection(name) do
    case Tesla.post(write_client(), "/api/v1/projections/#{name}/start", %{}) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: 404}} ->
        {:error, :not_found}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Create a new projection"
  def create_projection(projection) when is_map(projection) do
    case Tesla.post(write_client(), "/api/v1/projections", projection) do
      {:ok, %Tesla.Env{status: 201, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  ## Schemas

  @doc "List all schemas"
  def list_schemas do
    case Tesla.get(read_client(), "/api/v1/schemas") do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Get schema for event type"
  def get_schema(event_type, version \\ nil) do
    path =
      if version do
        "/api/v1/schemas/#{event_type}?version=#{version}"
      else
        "/api/v1/schemas/#{event_type}"
      end

    case Tesla.get(read_client(), path) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: 404}} ->
        {:error, :not_found}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Register a new schema"
  def register_schema(schema) when is_map(schema) do
    case Tesla.post(write_client(), "/api/v1/schemas", schema) do
      {:ok, %Tesla.Env{status: 201, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  ## Demo

  @doc "Seed demo data in Core. Returns {:ok, body} on success."
  def seed_demo do
    case Tesla.post(write_client(), "/api/v1/demo/seed", %{}) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  ## Snapshots

  @doc "List snapshots"
  def list_snapshots(entity_id \\ nil) do
    path =
      if entity_id do
        "/api/v1/snapshots?entity_id=#{entity_id}"
      else
        "/api/v1/snapshots"
      end

    case Tesla.get(read_client(), path) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Create a snapshot"
  def create_snapshot(entity_id, snapshot_type) do
    case Tesla.post(write_client(), "/api/v1/snapshots", %{
           entity_id: entity_id,
           snapshot_type: snapshot_type
         }) do
      {:ok, %Tesla.Env{status: 201, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  ## Projection State (v0.7 Core Integration)

  @doc """
  Get projection state for an entity from Core's DashMap.

  ## Parameters
    * `projection_name` - Name of the projection (e.g., "entity_snapshots")
    * `entity_id` - Entity identifier

  ## Returns
    * `{:ok, state}` - Projection state as map
    * `{:error, :not_found}` - No state found
    * `{:error, reason}` - Error details
  """
  def get_projection_state(projection_name, entity_id) do
    case Tesla.get(read_client(), "/api/v1/projections/#{projection_name}/#{entity_id}/state") do
      {:ok, %Tesla.Env{status: 200, body: %{"found" => true, "state" => state}}} ->
        {:ok, state}

      {:ok, %Tesla.Env{status: 200, body: %{"found" => false}}} ->
        {:error, :not_found}

      {:ok, %Tesla.Env{status: 404}} ->
        {:error, :not_found}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  Save projection state for an entity to Core's DashMap.

  ## Parameters
    * `projection_name` - Name of the projection
    * `entity_id` - Entity identifier
    * `state` - State to save (map)

  ## Returns
    * `:ok` - State saved successfully
    * `{:error, reason}` - Error details
  """
  def save_projection_state(projection_name, entity_id, state) when is_map(state) do
    case Tesla.put(write_client(), "/api/v1/projections/#{projection_name}/#{entity_id}/state", %{
           state: state
         }) do
      {:ok, %Tesla.Env{status: 200, body: %{"saved" => true}}} ->
        :ok

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  Bulk get projection states for multiple entities.

  ## Parameters
    * `projection_name` - Name of the projection
    * `entity_ids` - List of entity identifiers

  ## Returns
    * `{:ok, states}` - List of %{entity_id, state, found}
    * `{:error, reason}` - Error details
  """
  def bulk_get_projection_states(projection_name, entity_ids) when is_list(entity_ids) do
    case Tesla.post(read_client(), "/api/v1/projections/#{projection_name}/bulk", %{
           entity_ids: entity_ids
         }) do
      {:ok, %Tesla.Env{status: 200, body: %{"states" => states}}} ->
        {:ok, states}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  Bulk save projection states for multiple entities.

  ## Parameters
    * `states` - List of %{projection_name, entity_id, state}

  ## Returns
    * `:ok` - All states saved
    * `{:error, reason}` - Error details
  """
  def bulk_save_projection_states(states) when is_list(states) do
    # Save states sequentially (could be parallelized with Task.async_stream)
    results =
      Enum.map(states, fn %{projection_name: name, entity_id: id, state: state} ->
        save_projection_state(name, id, state)
      end)

    if Enum.all?(results, &(&1 == :ok)) do
      :ok
    else
      {:error, {:partial_failure, results}}
    end
  end

  ## API Key Verification

  @doc """
  Verify a customer API key.

  API keys are self-contained JWTs signed with the shared `JWT_SECRET` (the same
  secret Core and the Control Plane hold). We validate them **locally** — HS256
  signature plus expiry — exactly like the `Authorization: Bearer` path in
  `AuthPipeline`/`JwtAuth`, then resolve the tenant from Core for the
  subscription gate.

  This deliberately does NOT round-trip to Core's `/api/v1/auth/me`. That handler
  decodes into Core's `Claims` struct, which has a required `iss` field and
  enforces `set_issuer("allsource")`. Keys minted by the Query Service
  (`ApiKeyController.sign_api_key/3`) omitted `iss`, so every such key failed
  Core's validation and the gateway returned 401 "invalid API key" on every data
  endpoint — even though the key was valid and correctly signed. Local validation
  also removes a network hop from the hot auth path and loses nothing: Core's
  `/auth/me` validated the JWT statelessly anyway (it has no record of
  gateway-minted keys), so there was no revocation check to preserve.

  Returns `{:ok, {tenant_map, api_key_info}}` or `{:error, reason}`.
  """
  def verify_api_key(raw_key) when is_binary(raw_key) do
    case decode_api_key_jwt(raw_key) do
      {:ok, claims} ->
        tenant_id = claims["tenant_id"]

        case get_tenant(tenant_id) do
          {:ok, tenant} ->
            api_key_info = %{
              "id" => claims["sub"],
              "name" => claims["name"],
              "tenant_id" => tenant_id,
              "role" => claims["role"] || "serviceaccount",
              "scopes" => claims["scopes"] || []
            }

            {:ok, {tenant, api_key_info}}

          {:error, :not_found} ->
            {:error, :invalid_key}

          {:error, _reason} ->
            {:error, :invalid_key}
        end

      {:error, reason} ->
        {:error, reason}
    end
  end

  # Locally validate an API-key JWT: HS256 signature against JWT_SECRET, then
  # presence/expiry of the standard claims. Mirrors `JwtAuth.verify_and_decode/1`
  # but returns the API-key error atoms the caller already handles.
  #
  # Public only so the auth contract can be unit-tested directly (no Core round
  # trip). Not part of the supported API — call `verify_api_key/1`.
  @doc false
  def decode_api_key_jwt(raw_key) do
    case System.get_env("JWT_SECRET") do
      nil ->
        Logger.error("[RustCoreClient] JWT_SECRET not configured; cannot verify API keys")
        {:error, :invalid_key}

      secret ->
        jwk = JOSE.JWK.from_oct(secret)

        case JOSE.JWT.verify_strict(jwk, ["HS256"], raw_key) do
          {true, %JOSE.JWT{fields: claims}, _jws} ->
            validate_api_key_claims(claims)

          {false, _jwt, _jws} ->
            {:error, :invalid_key}

            # JOSE.JWT.verify_strict/3 only ever returns {boolean(), JWT, JWS};
            # an {:error, _} clause here is unreachable. Any other shape raises,
            # and the rescue below turns that into {:error, :invalid_key} anyway.
        end
    end
  rescue
    # JOSE raises on structurally invalid tokens (e.g. not three base64 segments).
    _ -> {:error, :invalid_key}
  end

  defp validate_api_key_claims(claims) do
    now = System.system_time(:second)

    cond do
      not is_integer(claims["exp"]) -> {:error, :invalid_key}
      claims["exp"] <= now -> {:error, :key_expired}
      not is_binary(claims["tenant_id"]) -> {:error, :invalid_key}
      not is_binary(claims["sub"]) -> {:error, :invalid_key}
      true -> {:ok, claims}
    end
  end

  ## Tenants

  @doc """
  Get a tenant from Core by ID.

  Returns the tenant as a map with `"id"`, `"name"`, `"status"`, and `"metadata"` keys.

  ## Returns
    * `{:ok, tenant}` - Tenant map from Core
    * `{:error, :not_found}` - Tenant does not exist
    * `{:error, reason}` - Error details
  """
  def get_tenant(tenant_id) when is_binary(tenant_id) do
    fetch_tenant(read_client(), tenant_id)
  end

  @doc """
  Read current tenant metadata from the leader for credential verification.

  No follower, read cache or retry can supply stale revocation state. A five-second
  request timeout bounds failure; callers must deny access if this read fails.
  """
  @spec get_tenant_for_authorization(String.t()) :: {:ok, map()} | {:error, term()}
  def get_tenant_for_authorization(tenant_id)
      when is_binary(tenant_id) and byte_size(tenant_id) in 1..128 do
    if Regex.match?(~r/\A[A-Za-z0-9_-]+\z/, tenant_id),
      do: fetch_tenant(authorization_client(), tenant_id),
      else: {:error, :invalid_tenant}
  end

  def get_tenant_for_authorization(_), do: {:error, :invalid_tenant}

  @doc "Read an admin-only system config value from the leader, without caching or retry."
  @spec get_config_for_authorization(String.t()) :: {:ok, term()} | {:error, atom()}
  def get_config_for_authorization(key) do
    if valid_authorization_config_key?(key) do
      fetch_config_for_authorization(key)
    else
      {:error, :invalid_key}
    end
  end

  @doc "Read Control Plane's current member list from the leader; no write counterpart."
  @spec get_team_members_for_authorization(term()) :: {:ok, term()} | {:error, atom()}
  def get_team_members_for_authorization(tenant_id)
      when is_binary(tenant_id) and byte_size(tenant_id) in 1..128 do
    if Regex.match?(~r/\A[A-Za-z0-9_-]+\z/, tenant_id),
      do: fetch_config_for_authorization("team:#{tenant_id}:members"),
      else: {:error, :invalid_tenant}
  end

  def get_team_members_for_authorization(_), do: {:error, :invalid_tenant}

  defp fetch_config_for_authorization(key) do
    case Tesla.get(authorization_client(), "/api/v1/config/#{key}") do
      {:ok, %Tesla.Env{status: 200, body: %{"key" => ^key, "value" => value}}} -> {:ok, value}
      {:ok, %Tesla.Env{status: 404}} -> {:error, :not_found}
      _ -> {:error, :storage_unavailable}
    end
  end

  @doc "Persist an opaque security config value through Core's existing admin-only API."
  @spec put_config_for_authorization(String.t(), map()) :: {:ok, map()} | {:error, atom()}
  def put_config_for_authorization(key, value) do
    if valid_authorization_config_key?(key) and is_map(value) do
      body = %{key: key, value: value, changed_by: "customer-agent-service"}

      case Tesla.post(authorization_client(), "/api/v1/config", body) do
        {:ok, %Tesla.Env{status: 200, body: %{"key" => ^key, "saved" => true} = response}} ->
          {:ok, response}

        _ ->
          {:error, :storage_unavailable}
      end
    else
      {:error, :invalid_key}
    end
  end

  defp valid_authorization_config_key?(key) when is_binary(key) and byte_size(key) in 1..128,
    do: Regex.match?(~r/\Acustomer_agent_v1\.(?:grant|revoked)\.[0-9a-f]{32}\z/, key)

  defp valid_authorization_config_key?(_), do: false

  @doc "Read the bounded customer connection registry and its durable Core revision."
  def get_customer_connection_registry(tenant) do
    with {:ok, key} <- connection_registry_key(tenant) do
      case Tesla.get(authorization_client(), "/api/v1/config/#{key}") do
        {:ok,
         %Tesla.Env{status: 200, body: %{"key" => ^key, "value" => value, "revision" => revision}}} ->
          if valid_config_revision?(revision),
            do: {:ok, value, revision},
            else: {:error, :storage_unavailable}

        {:ok, %Tesla.Env{status: 404}} ->
          {:error, :not_found}

        _ ->
          {:error, :storage_unavailable}
      end
    end
  end

  @doc "Commit connections and consent together; old Core versions must fail closed."
  def put_customer_connection_registry(tenant, value, revision) do
    with {:ok, key} <- connection_registry_key(tenant),
         true <- is_nil(revision) or valid_config_revision?(revision),
         true <- is_map(value),
         {:ok, encoded} <- Jason.encode(value),
         true <- byte_size(encoded) <= 60_000 do
      condition =
        if is_nil(revision), do: %{kind: "absent"}, else: %{kind: "revision", revision: revision}

      body = %{key: key, value: value, condition: condition, changed_by: "customer-agent-service"}

      case Tesla.post(authorization_client(), "/api/v1/config/conditional/set", body) do
        {:ok,
         %Tesla.Env{status: 200, body: %{"key" => ^key, "saved" => true, "revision" => next}}} ->
          if valid_config_revision?(next) and next != revision,
            do: :ok,
            else: {:error, :storage_unavailable}

        {:ok,
         %Tesla.Env{
           status: 409,
           body: %{"error" => "Concurrency error: Configuration precondition failed"}
         }} ->
          {:error, :conflict}

        _ ->
          {:error, :storage_unavailable}
      end
    else
      _ -> {:error, :storage_unavailable}
    end
  end

  defp connection_registry_key(tenant) when is_binary(tenant) and byte_size(tenant) in 1..128 do
    if Regex.match?(~r/\A[A-Za-z0-9_-]+\z/, tenant) do
      hash = :sha256 |> :crypto.hash(tenant) |> Base.encode16(case: :lower)
      {:ok, "customer_agent_v2.connections." <> hash}
    else
      {:error, :invalid_tenant}
    end
  end

  defp connection_registry_key(_), do: {:error, :invalid_tenant}

  defp valid_config_revision?(revision) when is_binary(revision),
    do:
      Regex.match?(~r/\A[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\z/, revision)

  defp valid_config_revision?(_), do: false

  defp authorization_client do
    url =
      Application.get_env(:query_service_ex, :core_write_url) ||
        Application.get_env(:query_service_ex, :core_url, @default_base_url)

    build_client(url, timeout: 5_000, max_retries: 0)
  end

  defp fetch_tenant(client, tenant_id) do
    case Tesla.get(client, "/api/v1/tenants/#{tenant_id}") do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: 404}} ->
        {:error, :not_found}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  Set a tenant's schema-enforcement mode in Core (Gap 3 toggle).

  `mode` is one of "permissive", "warn", "strict". Core's endpoint is
  admin-gated; the gateway scopes this to the authenticated tenant.
  """
  def set_schema_enforcement(tenant_id, mode)
      when is_binary(tenant_id) and is_binary(mode) do
    body = %{schema_enforcement: mode}

    case Tesla.put(write_client(), "/api/v1/tenants/#{tenant_id}/schema-enforcement", body) do
      {:ok, %Tesla.Env{status: status}} when status in [200, 204] ->
        {:ok, mode}

      {:ok, %Tesla.Env{status: 404}} ->
        {:error, :not_found}

      {:ok, %Tesla.Env{status: status, body: resp_body}} ->
        {:error, "HTTP #{status}: #{inspect(resp_body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  Deep-merge a partial metadata map into a tenant's metadata in Core.

  Core's `PATCH /api/v1/tenants/{id}/metadata` performs a tenant-scoped atomic
  deep-merge, so sibling keys (e.g. `metadata.quotas`) are preserved. Core never
  interprets the keys it stores — they are opaque to the engine. Used by the
  per-tenant projections feature to persist `metadata.projections.enabled`.

  ## Parameters
    * `tenant_id` - The tenant whose metadata to merge into
    * `partial` - A map merged into the tenant's existing metadata

  ## Returns
    * `{:ok, body}` - Merge applied (Core may echo the merged tenant/metadata)
    * `{:error, :not_found}` - Tenant does not exist
    * `{:error, reason}` - Error details
  """
  def merge_tenant_metadata(tenant_id, partial)
      when is_binary(tenant_id) and is_map(partial) do
    case Tesla.patch(write_client(), "/api/v1/tenants/#{tenant_id}/metadata", partial) do
      {:ok, %Tesla.Env{status: status, body: body}} when status in [200, 204] ->
        {:ok, body}

      {:ok, %Tesla.Env{status: 404}} ->
        {:error, :not_found}

      {:ok, %Tesla.Env{status: status, body: resp_body}} ->
        {:error, "HTTP #{status}: #{inspect(resp_body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  Create a tenant in Core.

  Used for lazy auto-provisioning when a valid JWT references a tenant that
  doesn't yet exist in Core (e.g., after Core restart or race condition).

  ## Parameters
    * `tenant_id` - The tenant ID to create
    * `name` - Display name for the tenant
    * `opts` - Optional map with `is_demo`, `email`, `tier` keys

  ## Returns
    * `{:ok, tenant}` - Newly created tenant map
    * `{:error, :conflict}` - Tenant already exists (409)
    * `{:error, reason}` - Error details
  """
  def create_tenant(tenant_id, name, opts \\ %{}) when is_binary(tenant_id) do
    is_demo = Map.get(opts, :is_demo, false)
    tier = Map.get(opts, :tier, "free")

    body = %{
      "id" => tenant_id,
      "name" => name,
      "is_demo" => is_demo,
      "quota_preset" => tier
    }

    case Tesla.post(write_client(), "/api/v1/tenants", body) do
      {:ok, %Tesla.Env{status: status, body: resp_body}} when status in [200, 201] ->
        {:ok, resp_body}

      {:ok, %Tesla.Env{status: 409}} ->
        # Tenant already exists — fetch and return it
        get_tenant(tenant_id)

      {:ok, %Tesla.Env{status: status, body: resp_body}} ->
        {:error, "HTTP #{status}: #{inspect(resp_body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  Delete a tenant.

  ## Returns
    * `:ok` - Tenant deleted
    * `{:error, :not_found}` - Tenant does not exist
    * `{:error, reason}` - Error details
  """
  def delete_tenant(tenant_id) when is_binary(tenant_id) do
    case Tesla.delete(write_client(), "/api/v1/tenants/#{tenant_id}") do
      {:ok, %Tesla.Env{status: status}} when status in [200, 204] ->
        :ok

      {:ok, %Tesla.Env{status: 404}} ->
        {:error, :not_found}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  Trigger demo data seeding in Core. Idempotent — skips if already seeded.

  ## Returns
    * `{:ok, result}` - Seed result map
    * `{:error, reason}` - Error details
  """
  def demo_seed do
    case Tesla.post(write_client(), "/api/v1/demo/seed", %{}) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  ## Metrics & Health

  @doc "Get system metrics"
  def get_metrics do
    case Tesla.get(read_client(), "/metrics") do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  Get raw Prometheus text metrics from Core's /metrics endpoint.

  Uses a plain HTTP client (no JSON middleware) to preserve the raw
  Prometheus text format for parsing by PrometheusParser.
  """
  def get_metrics_raw do
    base_url =
      Application.get_env(:query_service_ex, :core_url, @default_base_url)

    middleware = [
      {Tesla.Middleware.BaseUrl, base_url},
      {Tesla.Middleware.Timeout, timeout: @default_timeout}
    ]

    # Add Authorization header if CORE_API_KEY is configured
    middleware =
      case Application.get_env(:query_service_ex, :core_api_key) do
        nil -> middleware
        "" -> middleware
        api_key -> [{Tesla.Middleware.Headers, [{"authorization", api_key}]} | middleware]
      end

    client = Tesla.client(middleware, {Tesla.Adapter.Hackney, [connect_options: [:inet6]]})

    case Tesla.get(client, "/metrics") do
      {:ok, %Tesla.Env{status: 200, body: body}} when is_binary(body) ->
        {:ok, body}

      {:ok, %Tesla.Env{status: 200, body: body}} ->
        # Body was decoded somehow — convert back to string
        {:ok, inspect(body)}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Check health status"
  def health_check do
    case Tesla.get(read_client(), "/health") do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status}} ->
        {:error, "HTTP #{status}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  ## Private Helpers

  defp compile_query(%Query{} = query) do
    params = flatten_predicate(query.where, %{})

    params
    |> maybe_put(:limit, query.limit)
    |> maybe_put(:offset, query.offset)
  end

  # Flatten a predicate tree into Core-compatible flat query params.
  # Core accepts: entity_id, event_type, since, until, as_of, limit, offset.
  defp flatten_predicate(nil, acc), do: acc

  defp flatten_predicate(%{operator: :eq, field: field, value: value}, acc)
       when field in [:entity_id, :event_type, :since, :until, :as_of] do
    Map.put(acc, field, value)
  end

  # Ignore unsupported predicates (Core doesn't support arbitrary field filters)
  defp flatten_predicate(%{operator: :eq, field: _field}, acc), do: acc

  # Flatten compound predicates — extract all supported fields from children
  defp flatten_predicate(%{operator: op, value: children}, acc)
       when op in [:and, :or] and is_list(children) do
    Enum.reduce(children, acc, &flatten_predicate/2)
  end

  # Catch-all for unsupported predicate types
  defp flatten_predicate(_predicate, acc), do: acc

  defp maybe_put(params, _key, nil), do: params
  defp maybe_put(params, key, value), do: Map.put(params, key, value)

  ## Webhook Management

  @doc """
  Register a new webhook subscription.
  """
  def register_webhook(tenant_id, params) when is_binary(tenant_id) and is_map(params) do
    body = Map.put(params, :tenant_id, tenant_id)

    case Tesla.post(write_client(), "/api/v1/webhooks", body) do
      {:ok, %Tesla.Env{status: 200, body: %{"webhook" => webhook}}} ->
        {:ok, webhook}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  List webhooks for a tenant.
  """
  def list_webhooks(tenant_id) when is_binary(tenant_id) do
    case Tesla.get(read_client(), "/api/v1/webhooks", query: [tenant_id: tenant_id]) do
      {:ok, %Tesla.Env{status: 200, body: %{"webhooks" => webhooks, "total" => total}}} ->
        {:ok, %{webhooks: webhooks, total: total}}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  Get a specific webhook by ID.
  """
  def get_webhook(webhook_id) when is_binary(webhook_id) do
    case Tesla.get(read_client(), "/api/v1/webhooks/#{webhook_id}") do
      {:ok, %Tesla.Env{status: 200, body: %{"webhook" => webhook}}} ->
        {:ok, webhook}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  Update a webhook subscription.
  """
  def update_webhook(webhook_id, params) when is_binary(webhook_id) and is_map(params) do
    case Tesla.put(write_client(), "/api/v1/webhooks/#{webhook_id}", params) do
      {:ok, %Tesla.Env{status: 200, body: %{"webhook" => webhook}}} ->
        {:ok, webhook}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  Delete a webhook subscription.
  """
  def delete_webhook(webhook_id) when is_binary(webhook_id) do
    case Tesla.delete(write_client(), "/api/v1/webhooks/#{webhook_id}") do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  List delivery history for a webhook.
  """
  def list_webhook_deliveries(webhook_id, opts \\ []) when is_binary(webhook_id) do
    limit = Keyword.get(opts, :limit, 50)

    case Tesla.get(read_client(), "/api/v1/webhooks/#{webhook_id}/deliveries",
           query: [limit: limit]
         ) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  # -------------------------------------------------------------------
  # Replay Operations
  # -------------------------------------------------------------------

  @doc """
  Start a new event replay.
  """
  def start_replay(params) when is_map(params) do
    case Tesla.post(write_client(), "/api/v1/replay", params) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  List all replays.
  """
  def list_replays do
    case Tesla.get(read_client(), "/api/v1/replay") do
      {:ok, %Tesla.Env{status: 200, body: %{"replays" => replays, "total" => total}}} ->
        {:ok, %{replays: replays, total: total}}

      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  Get replay progress by ID.
  """
  def get_replay(replay_id) when is_binary(replay_id) do
    case Tesla.get(read_client(), "/api/v1/replay/#{replay_id}") do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  Cancel a running replay.
  """
  def cancel_replay(replay_id) when is_binary(replay_id) do
    case Tesla.post(write_client(), "/api/v1/replay/#{replay_id}/cancel", %{}) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  Delete a completed or failed replay.
  """
  def delete_replay(replay_id) when is_binary(replay_id) do
    case Tesla.delete(write_client(), "/api/v1/replay/#{replay_id}") do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  ## Prime — declarative projections + per-field provenance
  ##
  ## Proxies Core's /api/v1/prime/* projection routes (added in t-2ac8) so
  ## SDK/REST callers reach the same primitives the MCP tools expose. Core
  ## stays internal-only; the gateway gates access via :tenant_scoped.

  @doc "List declarative projection definitions registered in Prime"
  def list_prime_projections do
    case Tesla.get(read_client(), "/api/v1/prime/projections") do
      {:ok, %Tesla.Env{status: 200, body: %{"projections" => projections}}} ->
        {:ok, projections}

      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Define (or replace) a declarative projection for an entity type"
  def create_prime_projection(projection) when is_map(projection) do
    case Tesla.post(write_client(), "/api/v1/prime/projections", projection) do
      {:ok, %Tesla.Env{status: 201, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Fold a node's observation history into a snapshot via its projection"
  def project_node(node_id) when is_binary(node_id) do
    case Tesla.post(write_client(), "/api/v1/prime/nodes/#{node_id}/project", %{}) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  Fetch the COMPLETE materialized Prime knowledge graph from Core's EMBEDDED
  Prime store.

  > #### Not used by the hosted graph endpoint {: .warning}
  >
  > Core's embedded Prime store is single-tenant (everything ingested with
  > `tenant_id: None`) and SEPARATE from the main multi-tenant event store.
  > Synced tenant memory (`prime.*` events) lives in the MAIN store, so this
  > would return EMPTY for real tenants. `PrimeController.graph/2` therefore
  > materializes the graph from the tenant's `prime.*` events via
  > `QueryServiceEx.Prime.GraphFold` instead. Kept for embedded/local use only.

  Proxies Core's `GET /api/v1/prime/graph`, forwarding the authenticated
  tenant as `?tenant_id=` exactly like `query_events/3` does for events.

  `opts` accepts `:node_type` and `:limit`, mapped to Core's query params.
  Returns `{:ok, %{"nodes" => ..., "edges" => ..., "stats" => ..., "has_more" => ...}}`.
  """
  def get_prime_graph(tenant_id, opts \\ []) when is_binary(tenant_id) do
    query =
      [tenant_id: tenant_id]
      |> maybe_kw(:node_type, opts[:node_type])
      |> maybe_kw(:limit, opts[:limit])

    case Tesla.get(read_client(), "/api/v1/prime/graph", query: query) do
      {:ok, %Tesla.Env{status: 200, body: body}} when is_map(body) ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  defp maybe_kw(kw, _key, nil), do: kw
  defp maybe_kw(kw, _key, ""), do: kw
  defp maybe_kw(kw, key, value), do: Keyword.put(kw, key, value)

  @doc "Per-field provenance: which event supplied a node field's current value"
  def node_field_provenance(node_id, field)
      when is_binary(node_id) and is_binary(field) do
    case Tesla.get(
           read_client(),
           "/api/v1/prime/nodes/#{node_id}/fields/#{field}/provenance"
         ) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: 404}} ->
        {:error, :not_found}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end
end
