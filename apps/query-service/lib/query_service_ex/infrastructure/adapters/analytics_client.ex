defmodule QueryServiceEx.Infrastructure.Adapters.AnalyticsClient do
  @moduledoc """
  HTTP client adapter for Core's analytics endpoints.

  Provides access to:
  - Event frequency analysis with time-window aggregations
  - Statistical summaries
  - Event correlation analysis

  Results are cached in ETS with configurable TTL for performance.

  ## Authentication

  Uses the same `CORE_API_KEY` as RustCoreClient for authentication.

  ## Tenant Isolation

  All analytics queries support tenant_id filtering for multi-tenant isolation.
  """

  alias QueryServiceEx.Infrastructure.Adapters.AnalyticsCache
  alias QueryServiceEx.Infrastructure.Adapters.CoreHold

  @default_base_url "http://localhost:3900"
  @default_timeout 60_000

  # Time windows supported by Core
  @valid_windows ~w(minute hour day week month)

  @doc false
  def client do
    base_url = Application.get_env(:query_service_ex, :core_url, @default_base_url)

    middleware = [
      {Tesla.Middleware.BaseUrl, base_url},
      {CoreHold, max_retries: 3},
      Tesla.Middleware.JSON,
      {Tesla.Middleware.Timeout, timeout: @default_timeout}
    ]

    middleware =
      case Application.get_env(:query_service_ex, :core_api_key) do
        nil -> middleware
        "" -> middleware
        api_key -> [{Tesla.Middleware.Headers, [{"authorization", api_key}]} | middleware]
      end

    Tesla.client(middleware)
  end

  ## Event Frequency Analysis

  @doc """
  Analyze event frequency over time windows.

  ## Parameters
    * `tenant_id` - Tenant ID for isolation (optional)
    * `opts` - Keyword list of options:
      * `:entity_id` - Filter by entity ID
      * `:event_type` - Filter by event type
      * `:since` - Start time (ISO8601 string or DateTime, required)
      * `:until` - End time (ISO8601 string or DateTime, defaults to now)
      * `:window` - Time window: "minute", "hour", "day", "week", "month" (required)
      * `:use_cache` - Whether to use cache (default: true)
      * `:cache_ttl` - Cache TTL in seconds (default: 300)

  ## Returns
    * `{:ok, response}` - Frequency data with buckets
    * `{:error, reason}` - Error details

  ## Examples

      iex> event_frequency("tenant-123", since: "2026-02-01T00:00:00Z", window: "day")
      {:ok, %{
        "buckets" => [...],
        "total_events" => 1500,
        "window" => "day",
        "time_range" => %{"from" => "...", "to" => "..."}
      }}
  """
  def event_frequency(tenant_id \\ nil, opts) do
    with :ok <- validate_frequency_params(opts) do
      cache_key = build_cache_key(:frequency, tenant_id, opts)
      use_cache = Keyword.get(opts, :use_cache, true)
      cache_ttl = Keyword.get(opts, :cache_ttl, 300)

      if use_cache do
        AnalyticsCache.fetch(cache_key, cache_ttl, fn ->
          do_event_frequency(tenant_id, opts)
        end)
      else
        do_event_frequency(tenant_id, opts)
      end
    end
  end

  defp do_event_frequency(tenant_id, opts) do
    query_params =
      []
      |> maybe_add(:tenant_id, tenant_id)
      |> maybe_add(:entity_id, Keyword.get(opts, :entity_id))
      |> maybe_add(:event_type, Keyword.get(opts, :event_type))
      |> maybe_add(:since, format_datetime(Keyword.get(opts, :since)))
      |> maybe_add(:until, format_datetime(Keyword.get(opts, :until)))
      |> maybe_add(:window, Keyword.get(opts, :window))

    case Tesla.get(client(), "/api/v1/analytics/frequency", query: query_params) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  ## Statistical Summary

  @doc """
  Get comprehensive statistical summary of events.

  ## Parameters
    * `tenant_id` - Tenant ID for isolation (optional)
    * `opts` - Keyword list of options:
      * `:entity_id` - Filter by entity ID
      * `:event_type` - Filter by event type
      * `:since` - Start time (ISO8601 string or DateTime)
      * `:until` - End time (ISO8601 string or DateTime)
      * `:use_cache` - Whether to use cache (default: true)
      * `:cache_ttl` - Cache TTL in seconds (default: 600)

  ## Returns
    * `{:ok, summary}` - Statistical summary including:
      * `total_events` - Total event count
      * `unique_entities` - Number of unique entities
      * `unique_event_types` - Number of unique event types
      * `events_per_day` - Average events per day
      * `top_event_types` - Most common event types
      * `top_entities` - Most active entities
      * `first_event` / `last_event` - Time range
    * `{:error, reason}` - Error details
  """
  def stats_summary(tenant_id \\ nil, opts \\ []) do
    cache_key = build_cache_key(:summary, tenant_id, opts)
    use_cache = Keyword.get(opts, :use_cache, true)
    cache_ttl = Keyword.get(opts, :cache_ttl, 600)

    if use_cache do
      AnalyticsCache.fetch(cache_key, cache_ttl, fn ->
        do_stats_summary(tenant_id, opts)
      end)
    else
      do_stats_summary(tenant_id, opts)
    end
  end

  defp do_stats_summary(tenant_id, opts) do
    query_params =
      []
      |> maybe_add(:tenant_id, tenant_id)
      |> maybe_add(:entity_id, Keyword.get(opts, :entity_id))
      |> maybe_add(:event_type, Keyword.get(opts, :event_type))
      |> maybe_add(:since, format_datetime(Keyword.get(opts, :since)))
      |> maybe_add(:until, format_datetime(Keyword.get(opts, :until)))

    case Tesla.get(client(), "/api/v1/analytics/summary", query: query_params) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  ## Event Correlation

  @doc """
  Analyze correlation between two event types.

  Finds events of type A that are followed by events of type B within
  a specified time window, for the same entity.

  ## Parameters
    * `tenant_id` - Tenant ID for isolation (optional)
    * `event_type_a` - First event type (required)
    * `event_type_b` - Second event type (required)
    * `opts` - Keyword list of options:
      * `:time_window_seconds` - Max time between correlated events (default: 3600)
      * `:since` - Start time (ISO8601 string or DateTime)
      * `:until` - End time (ISO8601 string or DateTime)
      * `:use_cache` - Whether to use cache (default: true)
      * `:cache_ttl` - Cache TTL in seconds (default: 600)

  ## Returns
    * `{:ok, correlation}` - Correlation analysis including:
      * `total_a` / `total_b` - Total counts of each event type
      * `correlated_pairs` - Number of correlated event pairs
      * `correlation_percentage` - Percentage of A events that correlate with B
      * `avg_time_between_seconds` - Average time between correlated events
      * `examples` - Sample correlated event pairs
    * `{:error, reason}` - Error details

  ## Examples

      iex> correlation("tenant-123", "user.signup", "user.activated", time_window_seconds: 86400)
      {:ok, %{
        "event_type_a" => "user.signup",
        "event_type_b" => "user.activated",
        "correlated_pairs" => 450,
        "correlation_percentage" => 75.0,
        "avg_time_between_seconds" => 3600.5
      }}
  """
  def correlation(tenant_id \\ nil, event_type_a, event_type_b, opts \\ []) do
    cache_key =
      build_cache_key(:correlation, tenant_id, [{:a, event_type_a}, {:b, event_type_b} | opts])

    use_cache = Keyword.get(opts, :use_cache, true)
    cache_ttl = Keyword.get(opts, :cache_ttl, 600)

    if use_cache do
      AnalyticsCache.fetch(cache_key, cache_ttl, fn ->
        do_correlation(tenant_id, event_type_a, event_type_b, opts)
      end)
    else
      do_correlation(tenant_id, event_type_a, event_type_b, opts)
    end
  end

  defp do_correlation(tenant_id, event_type_a, event_type_b, opts) do
    query_params =
      []
      |> maybe_add(:tenant_id, tenant_id)
      |> maybe_add(:event_type_a, event_type_a)
      |> maybe_add(:event_type_b, event_type_b)
      |> maybe_add(:time_window_seconds, Keyword.get(opts, :time_window_seconds, 3600))
      |> maybe_add(:since, format_datetime(Keyword.get(opts, :since)))
      |> maybe_add(:until, format_datetime(Keyword.get(opts, :until)))

    case Tesla.get(client(), "/api/v1/analytics/correlation", query: query_params) do
      {:ok, %Tesla.Env{status: 200, body: body}} ->
        {:ok, body}

      {:ok, %Tesla.Env{status: status, body: body}} ->
        {:error, "HTTP #{status}: #{inspect(body)}"}

      {:error, reason} ->
        {:error, reason}
    end
  end

  ## Extended Analytics (computed in Query Service)

  @doc """
  Calculate percentile values for a numeric field in event payloads.

  This function queries events and computes percentiles locally.

  ## Parameters
    * `tenant_id` - Tenant ID for isolation
    * `opts` - Keyword list of options:
      * `:event_type` - Event type to analyze (required)
      * `:field` - JSON path to numeric field in payload (required)
      * `:percentiles` - List of percentiles to compute (default: [50, 90, 95, 99])
      * `:since` - Start time
      * `:until` - End time

  ## Returns
    * `{:ok, %{percentiles: %{50 => value, 90 => value, ...}}}` - Percentile values
    * `{:error, reason}` - Error details
  """
  def percentiles(tenant_id, opts) do
    event_type = Keyword.fetch!(opts, :event_type)
    field = Keyword.fetch!(opts, :field)
    percentile_list = Keyword.get(opts, :percentiles, [50, 90, 95, 99])

    with {:ok, events} <- fetch_events_for_analysis(tenant_id, opts) do
      values =
        events
        |> Enum.map(&extract_numeric_field(&1, field))
        |> Enum.reject(&is_nil/1)
        |> Enum.sort()

      if Enum.empty?(values) do
        {:error, :no_data}
      else
        percentile_results =
          percentile_list
          |> Enum.map(fn p -> {p, compute_percentile(values, p)} end)
          |> Map.new()

        {:ok,
         %{
           event_type: event_type,
           field: field,
           sample_size: length(values),
           percentiles: percentile_results
         }}
      end
    end
  end

  @doc """
  Calculate standard deviation and variance for a numeric field.

  ## Parameters
    * `tenant_id` - Tenant ID for isolation
    * `opts` - Keyword list of options:
      * `:event_type` - Event type to analyze (required)
      * `:field` - JSON path to numeric field in payload (required)
      * `:since` - Start time
      * `:until` - End time

  ## Returns
    * `{:ok, %{mean: float, stddev: float, variance: float, ...}}` - Statistics
    * `{:error, reason}` - Error details
  """
  def stddev(tenant_id, opts) do
    event_type = Keyword.fetch!(opts, :event_type)
    field = Keyword.fetch!(opts, :field)

    with {:ok, events} <- fetch_events_for_analysis(tenant_id, opts) do
      values =
        events
        |> Enum.map(&extract_numeric_field(&1, field))
        |> Enum.reject(&is_nil/1)

      if Enum.empty?(values) do
        {:error, :no_data}
      else
        n = length(values)
        mean = Enum.sum(values) / n
        variance = Enum.reduce(values, 0, fn x, acc -> acc + :math.pow(x - mean, 2) end) / n
        stddev_val = :math.sqrt(variance)

        {:ok,
         %{
           event_type: event_type,
           field: field,
           sample_size: n,
           mean: mean,
           variance: variance,
           stddev: stddev_val,
           min: Enum.min(values),
           max: Enum.max(values)
         }}
      end
    end
  end

  ## Time Window Aggregations (Enhanced)

  @doc """
  Perform tumbling window aggregation.

  Tumbling windows are fixed-size, non-overlapping time intervals.

  ## Parameters
    * `tenant_id` - Tenant ID for isolation
    * `opts` - Same as `event_frequency/2`

  ## Returns
    Same as `event_frequency/2` - each bucket represents a complete window.
  """
  def tumbling_window(tenant_id, opts) do
    # Tumbling windows are the default behavior of event_frequency
    event_frequency(tenant_id, opts)
  end

  @doc """
  Perform sliding window aggregation.

  Sliding windows overlap, providing smoothed time-series data.

  ## Parameters
    * `tenant_id` - Tenant ID for isolation
    * `opts` - Keyword list of options:
      * `:window` - Window size: "minute", "hour", "day", "week", "month"
      * `:slide` - Slide interval (same units as window, default: window/2)
      * `:since` - Start time (required)
      * `:until` - End time
      * Other options same as `event_frequency/2`

  ## Returns
    * `{:ok, %{buckets: [...], ...}}` - Overlapping window buckets
  """
  def sliding_window(tenant_id, opts) do
    window = Keyword.fetch!(opts, :window)
    slide = Keyword.get(opts, :slide, default_slide(window))

    # Get the base frequency data with smallest granularity
    base_window = smallest_window(window)
    base_opts = Keyword.put(opts, :window, base_window)

    with {:ok, base_data} <- event_frequency(tenant_id, Keyword.put(base_opts, :use_cache, false)) do
      buckets = base_data["buckets"] || []
      window_size = window_to_buckets(window, base_window)
      slide_size = window_to_buckets(slide, base_window)

      sliding_buckets = compute_sliding_windows(buckets, window_size, slide_size)

      {:ok,
       %{
         buckets: sliding_buckets,
         total_events: base_data["total_events"],
         window: window,
         slide: slide,
         time_range: base_data["time_range"]
       }}
    end
  end

  @doc """
  Perform session window aggregation.

  Session windows group events by activity sessions, with gaps defining session boundaries.

  ## Parameters
    * `tenant_id` - Tenant ID for isolation
    * `opts` - Keyword list of options:
      * `:entity_id` - Entity ID (required - sessions are per-entity)
      * `:gap_seconds` - Inactivity gap to start new session (default: 1800 = 30 min)
      * `:since` - Start time
      * `:until` - End time

  ## Returns
    * `{:ok, %{sessions: [...], total_sessions: n}}` - Session data
  """
  def session_window(tenant_id, opts) do
    entity_id = Keyword.fetch!(opts, :entity_id)
    gap_seconds = Keyword.get(opts, :gap_seconds, 1800)

    with {:ok, events} <-
           fetch_events_for_analysis(tenant_id, Keyword.put(opts, :entity_id, entity_id)) do
      sessions = group_into_sessions(events, gap_seconds)

      session_summaries =
        sessions
        |> Enum.with_index(1)
        |> Enum.map(fn {session_events, idx} ->
          %{
            session_id: idx,
            start_time: List.first(session_events)["timestamp"],
            end_time: List.last(session_events)["timestamp"],
            event_count: length(session_events),
            event_types: session_events |> Enum.map(& &1["event_type"]) |> Enum.frequencies(),
            duration_seconds: session_duration(session_events)
          }
        end)

      {:ok,
       %{
         entity_id: entity_id,
         sessions: session_summaries,
         total_sessions: length(sessions),
         avg_session_duration: avg_session_duration(session_summaries),
         avg_events_per_session: avg_events_per_session(session_summaries)
       }}
    end
  end

  ## Private Helpers

  defp validate_frequency_params(opts) do
    cond do
      !Keyword.has_key?(opts, :since) ->
        {:error, :missing_since}

      !Keyword.has_key?(opts, :window) ->
        {:error, :missing_window}

      Keyword.get(opts, :window) not in @valid_windows ->
        {:error, {:invalid_window, Keyword.get(opts, :window)}}

      true ->
        :ok
    end
  end

  defp build_cache_key(type, tenant_id, opts) do
    # Filter out cache-related options
    cache_opts = Keyword.drop(opts, [:use_cache, :cache_ttl])
    :erlang.phash2({type, tenant_id, cache_opts})
  end

  defp maybe_add(params, _key, nil), do: params
  defp maybe_add(params, key, value), do: [{key, value} | params]

  defp format_datetime(nil), do: nil
  defp format_datetime(%DateTime{} = dt), do: DateTime.to_iso8601(dt)
  defp format_datetime(str) when is_binary(str), do: str

  defp fetch_events_for_analysis(tenant_id, opts) do
    alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient

    query_params =
      %{}
      |> maybe_put(:entity_id, Keyword.get(opts, :entity_id))
      |> maybe_put(:event_type, Keyword.get(opts, :event_type))
      |> maybe_put(:since, format_datetime(Keyword.get(opts, :since)))
      |> maybe_put(:until, format_datetime(Keyword.get(opts, :until)))

    RustCoreClient.query_events(tenant_id, query_params)
  end

  defp maybe_put(map, _key, nil), do: map
  defp maybe_put(map, key, value), do: Map.put(map, key, value)

  defp extract_numeric_field(event, field) do
    payload = event["payload"] || event[:payload] || %{}

    case get_nested_field(payload, String.split(field, ".")) do
      value when is_number(value) -> value
      _ -> nil
    end
  end

  defp get_nested_field(data, []), do: data

  defp get_nested_field(data, [key | rest]) when is_map(data) do
    get_nested_field(Map.get(data, key) || Map.get(data, String.to_atom(key)), rest)
  end

  defp get_nested_field(_, _), do: nil

  defp compute_percentile(sorted_values, p) when p >= 0 and p <= 100 do
    n = length(sorted_values)
    rank = p / 100 * (n - 1)
    lower_idx = trunc(rank)
    upper_idx = min(lower_idx + 1, n - 1)
    fraction = rank - lower_idx

    lower_val = Enum.at(sorted_values, lower_idx)
    upper_val = Enum.at(sorted_values, upper_idx)

    lower_val + fraction * (upper_val - lower_val)
  end

  defp smallest_window(window) do
    case window do
      "month" -> "day"
      "week" -> "day"
      "day" -> "hour"
      "hour" -> "minute"
      "minute" -> "minute"
    end
  end

  defp default_slide(window) do
    # Default slide is half the window
    case window do
      "month" -> "week"
      "week" -> "day"
      "day" -> "hour"
      "hour" -> "minute"
      "minute" -> "minute"
    end
  end

  defp window_to_buckets(window, base) do
    units = %{
      "minute" => 1,
      "hour" => 60,
      "day" => 1_440,
      "week" => 10_080,
      "month" => 43_200
    }

    div(units[window], units[base])
  end

  defp compute_sliding_windows(buckets, window_size, slide_size) do
    buckets
    |> Enum.chunk_every(window_size, slide_size, :discard)
    |> Enum.map(fn window_buckets ->
      %{
        timestamp: List.first(window_buckets)["timestamp"],
        count: Enum.sum(Enum.map(window_buckets, & &1["count"])),
        event_types: merge_event_types(window_buckets)
      }
    end)
  end

  defp merge_event_types(buckets) do
    buckets
    |> Enum.flat_map(fn b -> Map.to_list(b["event_types"] || %{}) end)
    |> Enum.reduce(%{}, fn {k, v}, acc -> Map.update(acc, k, v, &(&1 + v)) end)
  end

  defp group_into_sessions(events, gap_seconds) do
    events
    |> Enum.sort_by(& &1["timestamp"])
    |> Enum.reduce([], fn event, sessions ->
      case sessions do
        [] ->
          [[event]]

        [current_session | rest] ->
          last_event = List.last(current_session)
          last_ts = parse_timestamp(last_event["timestamp"])
          current_ts = parse_timestamp(event["timestamp"])

          if DateTime.diff(current_ts, last_ts, :second) > gap_seconds do
            [[event], current_session | rest]
          else
            [current_session ++ [event] | rest]
          end
      end
    end)
    |> Enum.reverse()
  end

  defp parse_timestamp(ts) when is_binary(ts) do
    case DateTime.from_iso8601(ts) do
      {:ok, dt, _} -> dt
      _ -> DateTime.utc_now()
    end
  end

  defp parse_timestamp(%DateTime{} = dt), do: dt
  defp parse_timestamp(_), do: DateTime.utc_now()

  defp session_duration(events) when length(events) < 2, do: 0

  defp session_duration(events) do
    first_ts = parse_timestamp(List.first(events)["timestamp"])
    last_ts = parse_timestamp(List.last(events)["timestamp"])
    DateTime.diff(last_ts, first_ts, :second)
  end

  defp avg_session_duration([]), do: 0.0

  defp avg_session_duration(sessions) do
    total = Enum.sum(Enum.map(sessions, & &1.duration_seconds))
    total / length(sessions)
  end

  defp avg_events_per_session([]), do: 0.0

  defp avg_events_per_session(sessions) do
    total = Enum.sum(Enum.map(sessions, & &1.event_count))
    total / length(sessions)
  end
end
