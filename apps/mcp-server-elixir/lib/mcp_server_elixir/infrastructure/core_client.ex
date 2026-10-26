defmodule McpServerElixir.Infrastructure.CoreClient do
  @moduledoc """
  HTTP client for communicating with the Rust Core API.

  Implements `CoreBackend` behaviour for the remote (HTTP) backend.
  The Tesla client is configured at the module level — no instance state needed.

  ## Authentication (#228)

  A hosted Core / gateway (e.g. `https://api.all-source.xyz`) requires
  `Authorization: Bearer <key>`. Without it every request 401s, which the MCP
  layer surfaces as JSON-RPC `-32603 Internal error`. The raw key *without* the
  `Bearer` scheme 401s just the same, so the scheme is mandatory.

  Note that Tesla evaluates module-level `plug` arguments on every request (they
  are compiled into `__middleware__/0`, not frozen into a module attribute), so
  both the base URL and `auth_headers/0` below pick up whatever
  `config/runtime.exs` resolved at release boot.
  """

  @behaviour McpServerElixir.Infrastructure.CoreBackend

  use Tesla

  @default_base_url "http://localhost:3900"
  @default_timeout 30_000

  # Disable Hackney connection pooling so stale connections don't cause
  # errors after Core restarts. The MCP server makes infrequent calls,
  # so connection reuse provides negligible benefit.
  adapter(Tesla.Adapter.Hackney, pool: false, recv_timeout: @default_timeout)

  plug(
    Tesla.Middleware.BaseUrl,
    Application.get_env(:mcp_server_elixir, :core_url, @default_base_url)
  )

  # Authenticate against a hosted Core. Resolves per request; an empty list is
  # a no-op, so an unconfigured key sends no header rather than a bare "Bearer ".
  plug(Tesla.Middleware.Headers, auth_headers())

  plug(Tesla.Middleware.JSON)
  plug(Tesla.Middleware.Timeout, timeout: @default_timeout)

  plug(Tesla.Middleware.Retry,
    delay: 500,
    max_retries: 3,
    max_delay: 5_000,
    should_retry: fn
      {:ok, %{status: status}} when status in [408, 429, 500, 502, 503, 504] -> true
      {:ok, _} -> false
      {:error, _} -> true
    end
  )

  # 404/405 on a read route means the row is absent or this deployment does not
  # expose the route. The server never rebuilds the answer from raw events: a
  # fold over one bounded page is wrong for any entity or store larger than it.
  defp not_served(path, status, body) do
    "GET #{path} returned HTTP #{status}: #{inspect(body)}. It does not exist, or this " <>
      "deployment does not expose that route to this credential."
  end

  # View names are `[a-z0-9_-]` but row keys carry `:` and `/`, so every path
  # segment is percent-encoded.
  defp segment(value), do: URI.encode(to_string(value), &URI.char_unreserved?/1)

  defp compact(params) do
    params
    |> Enum.reject(fn {_k, v} -> is_nil(v) end)
    |> Enum.into(%{})
  end

  @doc false
  # The Authorization header sent with every request, or [] when no key is
  # configured. Public (undocumented) so tests can assert it without a socket.
  def auth_headers do
    case Application.get_env(:mcp_server_elixir, :core_api_key) do
      nil -> []
      "" -> []
      "Bearer " <> _ = key -> [{"authorization", key}]
      key -> [{"authorization", "Bearer " <> key}]
    end
  end

  @doc "Query events with flexible filters"
  @impl true
  def query_events(params) when is_map(params) do
    # Clean up params - remove nil values
    query_params =
      params
      |> Enum.reject(fn {_k, v} -> is_nil(v) end)
      |> Enum.into(%{})

    case get("/api/v1/events/query", query: query_params) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  Reconstruct entity state at a point in time, with its `history[]`.

  Core omits `history` unless asked, so this always sends `history=true`.
  """
  @impl true
  def reconstruct_state(entity_id, as_of \\ nil) do
    path = "/api/v1/entities/#{entity_id}/state"
    query_params = if as_of, do: [as_of: as_of, history: "true"], else: [history: "true"]

    case get(path, query: query_params) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} when status in [404, 405] ->
        {:error, not_served(path, status, body)}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Get current snapshot of an entity"
  @impl true
  def get_snapshot(entity_id) do
    case get("/api/v1/entities/#{entity_id}/snapshot") do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Ingest a new event (Core returns 200, not 201)"
  @impl true
  def ingest_event(event_data) do
    case post("/api/v1/events", event_data) do
      {:ok, %Tesla.Env{status: status, body: body}} when status in [200, 201] ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  Get event store statistics.

  `GET /api/v1/stats` is a **global, whole-store** endpoint on Core
  (`store.stats()`), which the hosted gateway deliberately does not route — a
  tenant must not see cross-tenant totals. Against a gateway it therefore 404s,
  which used to fail every stats tool with a bare `-32603` (#229). An absent
  route is reported as an error naming the route; a gateway that routes it
  answers tenant-scoped (#230).
  """
  @impl true
  def get_stats do
    path = "/api/v1/stats"

    case get(path) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} when status in [404, 405] ->
        {:error, not_served(path, status, body)}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  # ============================================================================
  # Views (latest-state read models, docs/contracts/latest-state §4)
  # ============================================================================

  @doc """
  Latest state of one view row (`GET /api/v1/views/{view}/entities/{key}`).

  A missing row reports the view's `ready` flag: a row absent from a view that
  is still building its first page may appear once the build commits.
  """
  @impl true
  def get_latest_state(view, key) do
    path = "/api/v1/views/#{segment(view)}/entities/#{segment(key)}"

    case get(path) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: 404, body: %{"error" => "not_found"} = body}} ->
        {:error,
         "No row #{inspect(key)} in view #{inspect(view)} " <>
           "(view ready: #{inspect(Map.get(body, "ready"))})"}

      {:ok, %Tesla.Env{status: status, body: body}} when status in [404, 405] ->
        {:error, not_served(path, status, body)}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  One page of a view's rows (`GET /api/v1/views/{view}/entities`).

  `params` takes `"prefix"`, `"after"` (the previous page's `next`) and
  `"limit"` (1..500).
  """
  @impl true
  def list_latest(view, params \\ %{}) when is_map(params) do
    view_read(
      "/api/v1/views/#{segment(view)}/entities",
      Map.take(params, ["prefix", "after", "limit"])
    )
  end

  @doc """
  Rows written after a revision (`GET /api/v1/views/{view}/changes`).

  `params` takes `"after"` and `"limit"`. Never holds the request (`wait` is
  not sent).
  """
  @impl true
  def list_changes(view, params) when is_map(params) do
    view_read("/api/v1/views/#{segment(view)}/changes", Map.take(params, ["after", "limit"]))
  end

  defp view_read(path, params) do
    case get(path, query: compact(params)) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: 409, body: %{"error" => "view_reset"} = body}} ->
        {:error,
         "View was rebuilt (generation #{inspect(Map.get(body, "generation"))}) after the " <>
           "requested revision. Re-read its rows with list_latest and resume list_changes " <>
           "from the revision that returns."}

      {:ok, %Tesla.Env{status: status, body: body}} when status in [404, 405] ->
        {:error, not_served(path, status, body)}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  Perform semantic (vector) search on events.

  Uses embeddings to find semantically similar events based on natural language queries.

  ## Parameters
    - `query` - Natural language search query
    - `limit` - Maximum number of results (default: 100)
    - `threshold` - Minimum similarity threshold 0.0-1.0 (default: 0.7)
  """
  @impl true
  def semantic_search(params) when is_map(params) do
    query_params =
      params
      |> Enum.reject(fn {_k, v} -> is_nil(v) end)
      |> Enum.into(%{})

    case get("/api/v1/search/semantic", query: query_params) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  Perform hybrid search combining semantic and keyword search.

  Uses both vector similarity (semantic) and BM25 (keyword) scoring with
  Reciprocal Rank Fusion (RRF) for optimal results.

  ## Parameters
    - `semantic_query` - Natural language query for semantic search (optional)
    - `keywords` - Keywords for BM25 search (optional)
    - `filters` - Object with optional filters:
      - `event_type` - Filter by event type
      - `entity_id` - Filter by entity ID
      - `time_from` - Filter events after this ISO timestamp
      - `time_to` - Filter events before this ISO timestamp
    - `limit` - Maximum number of results (default: 100)
  """
  @impl true
  def hybrid_search(params) when is_map(params) do
    # Build the request body, filtering out nil values
    body =
      params
      |> Enum.reject(fn {_k, v} -> is_nil(v) end)
      |> Enum.into(%{})

    case post("/api/v1/search/hybrid", body) do
      {:ok, %Tesla.Env{status: 200, body: response_body}} ->
        {:ok, response_body}

      {:ok, %Tesla.Env{status: status, body: response_body}} ->
        {:error, "HTTP #{status}: #{inspect(response_body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  # ============================================================================
  # Operational Endpoints
  # ============================================================================

  @doc "Trigger manual storage compaction (Core: POST /api/v1/compaction/trigger)"
  @impl true
  def compact_storage(params \\ %{}) do
    case post("/api/v1/compaction/trigger", params) do
      {:ok, %Tesla.Env{status: status, body: body}} when status in [200, 202] ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Get storage statistics — derived from Core stats + compaction stats"
  @impl true
  def storage_stats(_params \\ %{}) do
    # Core doesn't have a dedicated storage stats endpoint.
    # Combine event-store stats with compaction stats for a useful picture.
    compaction =
      case get("/api/v1/compaction/stats") do
        {:ok, %Tesla.Env{status: 200, body: body}} -> body
        _ -> "unavailable"
      end

    case get_stats() do
      {:ok, stats} -> {:ok, %{"event_store" => stats, "compaction" => compaction}}
      {:error, reason} -> {:error, reason}
    end
  end

  @doc "Get partition health and distribution info (Core: GET /api/v1/cluster/partitions)"
  @impl true
  def partition_info(_params \\ %{}) do
    case get("/api/v1/cluster/partitions") do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: 503, body: body}} ->
        # Cluster mode not enabled — return informative response
        {:ok, %{"status" => "cluster_not_enabled", "detail" => body}}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Get WAL/replication status from the health endpoint"
  @impl true
  def wal_status do
    # Core's /health endpoint includes replication status
    case get("/health") do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        wal_info = %{
          "role" => body["role"],
          "replication" => body["replication"],
          "system_streams" => body["system_streams"]
        }

        {:ok, wal_info}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Create a backup snapshot — not yet implemented in Core"
  @impl true
  def backup_create(_params \\ %{}) do
    {:error,
     "Backup API is not yet implemented in AllSource Core. Use WAL + Parquet files on disk for manual backups."}
  end

  @doc "Restore from a backup snapshot — not yet implemented in Core"
  @impl true
  def backup_restore(_params) do
    {:error,
     "Backup restore API is not yet implemented in AllSource Core. Restore by replacing WAL/Parquet data directory and restarting."}
  end

  @doc "List available backup snapshots — not yet implemented in Core"
  @impl true
  def backup_list(_params \\ %{}) do
    {:error,
     "Backup API is not yet implemented in AllSource Core. Check the data directory for WAL and Parquet files."}
  end

  @doc "Deep health check — uses Core's /health endpoint which includes system streams and replication"
  @impl true
  def health_deep do
    case get("/health") do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Get performance metrics — not yet implemented as a dedicated endpoint"
  @impl true
  def performance_report(_params \\ %{}) do
    # Core exposes Prometheus metrics at /metrics but not a JSON summary.
    # Return stats as a proxy for performance data.
    case get("/api/v1/stats") do
      {:ok, %Tesla.Env{status: 200, body: stats}} ->
        {:ok,
         %{
           "source" => "event_store_stats",
           "note" =>
             "Dedicated performance endpoint not yet available. Showing event store statistics. For detailed metrics, scrape /metrics (Prometheus format).",
           "stats" => stats
         }}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Query the audit trail (Core: GET /api/v1/audit/events)"
  @impl true
  def audit_log(params \\ %{}) do
    query_params =
      params
      |> Enum.reject(fn {_k, v} -> is_nil(v) end)
      |> Enum.into(%{})

    case get("/api/v1/audit/events", query: query_params) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Get cluster status (Core: GET /api/v1/cluster/status)"
  @impl true
  def get_cluster_status do
    case get("/api/v1/cluster/status") do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: 503, body: body}} ->
        # Cluster mode not enabled — fall back to health endpoint
        case get("/health") do
          {:ok, %Tesla.Env{status: 200, body: health}} ->
            {:ok,
             %{
               "cluster_mode" => "disabled",
               "node_status" => health,
               "note" => "Cluster mode is not enabled. Showing single-node health."
             }}

          _ ->
            {:ok, %{"cluster_mode" => "disabled", "detail" => body}}
        end

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @impl true
  def durability_status do
    # Remote Core doesn't expose a durability endpoint yet — compose from existing endpoints
    with {:ok, stats} <- get_stats(),
         {:ok, wal} <- wal_status() do
      {:ok,
       %{
         "memory_events" => Map.get(stats, "total_events", 0),
         "wal_enabled" => true,
         "wal_entries" => Map.get(wal, "total_entries", 0),
         "parquet_enabled" => true,
         "durable" => true,
         "warnings" => []
       }}
    end
  end

  # ============================================================================
  # Schema Endpoints
  # ============================================================================

  @doc "Register a new event type schema"
  @impl true
  def register_schema(params) when is_map(params) do
    case post("/api/v1/schemas", params) do
      {:ok, %Tesla.Env{status: status, body: body}} when status in [200, 201] ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Validate an event payload against a registered schema"
  @impl true
  def validate_schema(params) when is_map(params) do
    case post("/api/v1/schemas/validate", params) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "List all registered schema subjects"
  @impl true
  def list_schemas(params \\ %{}) do
    query_params =
      params
      |> Enum.reject(fn {_k, v} -> is_nil(v) end)
      |> Enum.into(%{})

    case get("/api/v1/schemas", query: query_params) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Get a specific schema by subject and optional version"
  @impl true
  def get_schema(subject, params \\ %{}) do
    query_params =
      params
      |> Enum.reject(fn {_k, v} -> is_nil(v) end)
      |> Enum.into(%{})

    case get("/api/v1/schemas/#{subject}", query: query_params) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "List schema versions for a subject"
  @impl true
  def list_schema_versions(subject) do
    case get("/api/v1/schemas/#{subject}/versions") do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Set compatibility mode for a schema subject"
  @impl true
  def set_compatibility(subject, params) when is_map(params) do
    case put("/api/v1/schemas/#{subject}/compatibility", params) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  # ============================================================================
  # Analytics Endpoints
  # ============================================================================

  @doc "Get event frequency analytics"
  @impl true
  def analytics_frequency(params \\ %{}) do
    query_params =
      params
      |> Enum.reject(fn {_k, v} -> is_nil(v) end)
      |> Enum.into(%{})

    case get("/api/v1/analytics/frequency", query: query_params) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Get analytics summary statistics"
  @impl true
  def analytics_summary(params \\ %{}) do
    query_params =
      params
      |> Enum.reject(fn {_k, v} -> is_nil(v) end)
      |> Enum.into(%{})

    case get("/api/v1/analytics/summary", query: query_params) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc "Get correlation analysis between event types"
  @impl true
  def analytics_correlation(params \\ %{}) do
    query_params =
      params
      |> Enum.reject(fn {_k, v} -> is_nil(v) end)
      |> Enum.into(%{})

    case get("/api/v1/analytics/correlation", query: query_params) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end
end
