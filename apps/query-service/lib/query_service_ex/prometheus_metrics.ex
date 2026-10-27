defmodule QueryServiceEx.PrometheusMetrics do
  @moduledoc """
  Prometheus metrics exporter for the Query Service.

  Defines and exposes metrics in Prometheus text format via the /metrics endpoint.
  Uses telemetry_metrics_prometheus_core to collect metrics from telemetry events
  emitted throughout the application. The `_core` package is the reporter only —
  it starts no HTTP listener of its own; the metrics are served by Phoenix
  (`/api/metrics/prometheus`) calling `scrape/0` below.

  ## Metric Categories

  ### HTTP Metrics
  - `http_request_duration_seconds` - Request latency histogram
  - `http_requests_total` - Total request count by status

  ### Event Processing Metrics
  - `events_processed_total` - Total events processed
  - `event_processing_duration_seconds` - Event processing latency

  ### Projection Metrics
  - `projection_updates_total` - Total projection updates
  - `projection_sync_duration_seconds` - Projection sync latency

  ### WebSocket Metrics
  - `websocket_connections_total` - Total WebSocket connections
  - `websocket_messages_received_total` - Messages received via WebSocket

  ### Database Metrics
  - `db_query_duration_seconds` - Database query latency

  ### Circuit Breaker Metrics
  - `circuit_breaker_state` - Current state of circuit breakers
  - `circuit_breaker_transitions_total` - State transition count

  ## APM Integration

  This module is compatible with:
  - DataDog (via Prometheus metrics endpoint)
  - NewRelic (via Prometheus integration)
  - Grafana Cloud
  - Any Prometheus-compatible monitoring system

  ## Usage

  The metrics are automatically exposed at GET /metrics in Prometheus text format.
  Configure your Prometheus server to scrape this endpoint.

  Example prometheus.yml:
  ```yaml
  scrape_configs:
    - job_name: 'query_service'
      static_configs:
        - targets: ['localhost:3902']
      metrics_path: '/metrics'
  ```
  """

  use Supervisor

  import Telemetry.Metrics

  @doc """
  Starts the Prometheus metrics exporter supervisor.
  """
  def start_link(opts) do
    Supervisor.start_link(__MODULE__, opts, name: __MODULE__)
  end

  @impl true
  def init(_opts) do
    children = [
      {TelemetryMetricsPrometheus.Core, metrics: metrics()}
    ]

    Supervisor.init(children, strategy: :one_for_one)
  end

  @doc """
  Returns the current metrics in Prometheus text format.

  This is used by the /metrics endpoint to return scraped metrics.
  """
  @spec scrape() :: String.t()
  def scrape do
    TelemetryMetricsPrometheus.Core.scrape()
  end

  @doc """
  Returns the list of metric definitions for the Query Service.

  These metrics are collected via Telemetry events and exposed in Prometheus format.
  """
  @spec metrics() :: [Telemetry.Metrics.t()]
  def metrics do
    http_metrics() ++
      event_processing_metrics() ++
      projection_metrics() ++
      websocket_metrics() ++
      database_metrics() ++
      circuit_breaker_metrics() ++
      vm_metrics() ++
      usage_reporter_metrics()
  end

  # HTTP/Phoenix request metrics
  defp http_metrics do
    [
      # Request duration histogram with percentiles
      distribution(
        "phoenix.endpoint.stop.duration",
        unit: {:native, :second},
        reporter_options: [buckets: [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10]],
        description: "HTTP request duration in seconds",
        tags: [:method, :route, :status]
      ),

      # Request count by status
      counter(
        "phoenix.endpoint.stop.count",
        description: "Total HTTP requests",
        tags: [:method, :route, :status]
      ),

      # HTTP errors
      counter(
        "query_service_ex.http.error.count",
        description: "Total HTTP errors",
        tags: [:error_type, :status]
      ),

      # No tenant tag: this endpoint is public. The log line names the tenant.
      counter(
        "query_service_ex.auth.key_authority_denial.count",
        description:
          "API-key requests the managed-key authority refused, or would refuse in shadow mode",
        tags: [:mode, :reason]
      )
    ]
  end

  # Event processing metrics
  defp event_processing_metrics do
    [
      # Events processed counter
      counter(
        "query_service_ex.event_pipeline.event_processed.count",
        description: "Total events processed",
        tags: [:event_type]
      ),

      # Event processing duration
      distribution(
        "query_service_ex.event_pipeline.event_processed.duration_ms",
        unit: :millisecond,
        reporter_options: [buckets: [1, 5, 10, 25, 50, 100, 250, 500, 1000]],
        description: "Event processing duration in milliseconds",
        tags: [:event_type]
      ),

      # Events failed counter
      counter(
        "query_service_ex.event_pipeline.events_failed.count",
        description: "Total events that failed processing",
        tags: []
      ),

      # Projections synced from events
      counter(
        "query_service_ex.event_pipeline.projections_synced.count",
        description: "Total projection sync operations from event pipeline",
        tags: []
      ),

      # Projection sync batch size
      distribution(
        "query_service_ex.event_pipeline.projections_synced.projection_count",
        unit: :unit,
        reporter_options: [buckets: [1, 2, 5, 10, 25, 50, 100]],
        description: "Number of projections synced per batch",
        tags: []
      )
    ]
  end

  # Projection metrics
  defp projection_metrics do
    [
      # Projection sync started
      counter(
        "query_service_ex.projection.sync_started.count",
        description: "Total projection sync operations started",
        tags: [:projection_name]
      ),

      # Projection sync completed
      counter(
        "query_service_ex.projection.sync_completed.count",
        description: "Total projection sync operations completed",
        tags: [:projection_name]
      ),

      # Projection sync duration
      distribution(
        "query_service_ex.projection.sync_completed.duration_ms",
        unit: :millisecond,
        reporter_options: [buckets: [1, 5, 10, 25, 50, 100, 250, 500, 1000]],
        description: "Projection sync duration in milliseconds",
        tags: [:projection_name]
      ),

      # Projection sync failures
      counter(
        "query_service_ex.projection.sync_failed.count",
        description: "Total projection sync failures",
        tags: [:projection_name]
      )
    ]
  end

  # WebSocket connection metrics
  defp websocket_metrics do
    [
      # WebSocket connections established
      counter(
        "query_service_ex.websocket.connected.count",
        description: "Total WebSocket connections established to Core",
        tags: []
      ),

      # WebSocket disconnections
      counter(
        "query_service_ex.websocket.disconnected.count",
        description: "Total WebSocket disconnections from Core",
        tags: []
      ),

      # WebSocket messages received
      counter(
        "query_service_ex.websocket.message_received.count",
        description: "Total WebSocket messages received",
        tags: [:message_type]
      ),

      # WebSocket message size
      distribution(
        "query_service_ex.websocket.message_received.size_bytes",
        unit: :byte,
        reporter_options: [buckets: [64, 256, 1024, 4096, 16_384, 65_536, 262_144]],
        description: "WebSocket message size in bytes",
        tags: [:message_type]
      ),

      # WebSocket reconnect attempts
      counter(
        "query_service_ex.websocket.reconnect_attempt.count",
        description: "Total WebSocket reconnection attempts",
        tags: []
      ),

      # WebSocket reconnect exhausted
      counter(
        "query_service_ex.websocket.reconnect_exhausted.count",
        description: "Total WebSocket reconnection failures (exhausted retries)",
        tags: []
      ),

      # Phoenix Channel connections (external clients)
      counter(
        "query_service_ex.channel.joined.count",
        description: "Total Phoenix Channel joins by external clients",
        tags: [:channel]
      ),

      # Phoenix Channel leaves
      counter(
        "query_service_ex.channel.left.count",
        description: "Total Phoenix Channel leaves by external clients",
        tags: [:channel]
      )
    ]
  end

  # Database query metrics (Ecto Repo removed — no longer applicable)
  defp database_metrics do
    []
  end

  # Circuit breaker metrics
  defp circuit_breaker_metrics do
    [
      # Circuit opened
      counter(
        "query_service_ex.circuit_breaker.opened.count",
        description: "Total circuit breaker open events",
        tags: []
      ),

      # Circuit closed
      counter(
        "query_service_ex.circuit_breaker.closed.count",
        description: "Total circuit breaker close events",
        tags: []
      ),

      # Circuit half-open
      counter(
        "query_service_ex.circuit_breaker.half_open.count",
        description: "Total circuit breaker half-open transitions",
        tags: []
      ),

      # Requests rejected by circuit breaker
      counter(
        "query_service_ex.circuit_breaker.rejected.count",
        description: "Total requests rejected by circuit breaker",
        tags: []
      ),

      # Health check completed
      counter(
        "query_service_ex.health_check.completed.count",
        description: "Total health check completions",
        tags: [:check, :status]
      ),

      # Health check timeout
      counter(
        "query_service_ex.health_check.timeout.count",
        description: "Total health check timeouts",
        tags: [:check]
      )
    ]
  end

  # Usage reporter metrics (async Core usage increments)
  defp usage_reporter_metrics do
    [
      counter(
        "query_service_ex.usage_reporter.flushed.count",
        description: "Total usage increments successfully flushed to Core",
        tags: [:tenant_id]
      ),
      counter(
        "query_service_ex.usage_reporter.dropped.count",
        description: "Total usage increments dropped after max retries",
        tags: [:tenant_id]
      )
    ]
  end

  # VM/runtime metrics from telemetry_poller
  defp vm_metrics do
    [
      # Memory usage
      last_value(
        "vm.memory.total",
        unit: :byte,
        description: "Total VM memory usage in bytes"
      ),
      last_value(
        "vm.memory.processes",
        unit: :byte,
        description: "Memory used by processes in bytes"
      ),
      last_value(
        "vm.memory.binary",
        unit: :byte,
        description: "Memory used by binaries in bytes"
      ),
      last_value(
        "vm.memory.ets",
        unit: :byte,
        description: "Memory used by ETS tables in bytes"
      ),
      last_value(
        "vm.memory.atom",
        unit: :byte,
        description: "Memory used by atoms in bytes"
      ),

      # Process count
      last_value(
        "vm.total_run_queue_lengths.total",
        description: "Total run queue length (scheduler workload)"
      ),
      last_value(
        "vm.total_run_queue_lengths.cpu",
        description: "CPU run queue length"
      ),
      last_value(
        "vm.total_run_queue_lengths.io",
        description: "IO run queue length"
      ),

      # System info
      last_value(
        "vm.system_counts.process_count",
        description: "Total number of processes"
      ),
      last_value(
        "vm.system_counts.atom_count",
        description: "Total number of atoms"
      ),
      last_value(
        "vm.system_counts.port_count",
        description: "Total number of ports"
      )
    ]
  end
end
