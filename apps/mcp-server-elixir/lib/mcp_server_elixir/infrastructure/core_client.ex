defmodule McpServerElixir.Infrastructure.CoreClient do
  @moduledoc """
  HTTP client for communicating with the Rust Core API.

  Implements `CoreBackend` behaviour for the remote (HTTP) backend.
  The Tesla client is configured at the module level — no instance state needed.

  ## Authentication (#228)

  A hosted Core / gateway (e.g. `https://api.all-source.xyz`) requires
  `Authorization: Bearer <credential>`. Without it every request 401s, which the
  MCP layer surfaces as JSON-RPC `-32603 Internal error`. `CoreCredentials`
  supplies the credential on every attempt: an ES256 client assertion when
  `ALLSOURCE_CLIENT_SIGNING_KEY` is set, otherwise the static key.

  Tesla evaluates module-level `plug` arguments on every request (they are
  compiled into `__middleware__/0`, not frozen into a module attribute), so the
  base URL picks up whatever `config/runtime.exs` resolved at release boot.
  """

  alias McpServerElixir.Infrastructure.CoreCredentials

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

  # After Retry so a retried attempt carries a credential minted for it. With no
  # credential configured no header is sent, rather than a bare "Bearer ".
  plug(CoreCredentials)

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
  # The Authorization header a request would carry now, or [] when none is
  # configured. Public (undocumented) so tests can assert it without a socket.
  def auth_headers, do: CoreCredentials.headers("authorization")

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

  @semantic_search_path "/api/v1/search/semantic"
  @semantic_search_params ["query", "limit", "event_type", "min_similarity"]
  @semantic_hit_keys ["event_id", "score", "event_type", "entity_id", "timestamp"]

  @hybrid_search_path "/api/v1/search/hybrid"
  @hybrid_search_params [
    "query",
    "limit",
    "event_type",
    "entity_id",
    "semantic_weight",
    "keyword_weight"
  ]
  @hybrid_hit_keys @semantic_hit_keys ++ ["semantic_score", "keyword_score"]

  @doc """
  Semantic (vector) search over events: `GET /api/v1/search/semantic`.

  Sends only `query` (required), `limit`, `event_type` and `min_similarity`
  as query parameters. The tenant comes from the credential, never a parameter.

  Returns `{:ok, %{"results" => hits, "count" => n}}`, each hit holding exactly
  `event_id`, `score`, `event_type`, `entity_id` and `timestamp`. Every non-200
  answer is an error, including 503 when Core's embedding engine or index is
  unavailable, and so is a 200 whose body does not have that shape.
  """
  @impl true
  def semantic_search(params) when is_map(params) do
    with {:ok, query} <- search_request(params, @semantic_search_params) do
      @semantic_search_path
      |> get(query: Map.to_list(query))
      |> decode_search("GET", @semantic_search_path, @semantic_hit_keys)
    end
  end

  @doc """
  Hybrid (semantic + keyword) search over events: `POST /api/v1/search/hybrid`.

  Sends a JSON body of `query` (required), `limit`, `event_type`, `entity_id`,
  `semantic_weight` and `keyword_weight`.

  Returns `{:ok, %{"results" => hits, "count" => n}}`, each hit holding exactly
  `event_id`, `score`, `semantic_score` and `keyword_score` (number or nil),
  `event_type`, `entity_id` and `timestamp`. Errors as `semantic_search/1`.
  """
  @impl true
  def hybrid_search(params) when is_map(params) do
    with {:ok, body} <- search_request(params, @hybrid_search_params) do
      @hybrid_search_path
      |> post(body)
      |> decode_search("POST", @hybrid_search_path, @hybrid_hit_keys)
    end
  end

  defp search_request(params, allowed) do
    case Map.get(params, "query") do
      query when is_binary(query) ->
        if String.trim(query) == "",
          do: {:error, "search query must not be empty"},
          else: {:ok, params |> Map.take(allowed) |> compact()}

      _ ->
        {:error, "search query is required"}
    end
  end

  defp decode_search({:ok, %Tesla.Env{status: 200, body: body}}, method, path, keys) do
    with %{"results" => results, "count" => count} when is_list(results) and is_integer(count) <-
           body,
         hits when is_list(hits) <- decode_hits(results, keys) do
      {:ok, %{"results" => hits, "count" => count}}
    else
      _ -> {:error, "#{method} #{path} returned an unexpected body: #{inspect(body)}"}
    end
  end

  defp decode_search({:ok, %Tesla.Env{status: 503, body: body}}, method, path, _keys) do
    {:error, "Core search is unavailable (#{method} #{path}, HTTP 503): #{error_detail(body)}"}
  end

  defp decode_search({:ok, %Tesla.Env{status: status, body: body}}, method, path, _keys) do
    {:error, "#{method} #{path} returned HTTP #{status}: #{error_detail(body)}"}
  end

  defp decode_search({:error, reason}, _method, _path, _keys), do: {:error, reason}

  defp decode_hits(results, keys) do
    Enum.reduce_while(results, [], fn
      hit, acc when is_map(hit) ->
        if Enum.all?(keys, &Map.has_key?(hit, &1)) and valid_hit?(hit),
          do: {:cont, [Map.take(hit, keys) | acc]},
          else: {:halt, :invalid}

      _hit, _acc ->
        {:halt, :invalid}
    end)
    |> case do
      :invalid -> :invalid
      hits -> Enum.reverse(hits)
    end
  end

  defp valid_hit?(hit) do
    is_binary(hit["event_id"]) and is_number(hit["score"]) and
      Enum.all?(["semantic_score", "keyword_score"], fn key ->
        is_nil(hit[key]) or is_number(hit[key])
      end)
  end

  defp error_detail(%{"error" => message}) when is_binary(message), do: message
  defp error_detail(body), do: inspect(body)

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

  @doc "Get performance metrics, answered from event store statistics"
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
             "Core has no JSON performance endpoint; these are event store statistics. For detailed metrics, scrape /metrics (Prometheus format).",
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
