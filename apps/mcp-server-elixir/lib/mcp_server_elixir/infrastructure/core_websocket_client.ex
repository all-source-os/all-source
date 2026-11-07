defmodule McpServerElixir.Infrastructure.CoreWebSocketClient do
  @moduledoc """
  WebSocket client for subscribing to real-time events from Rust Core.

  This GenServer connects to Core's `/api/v1/events/stream` WebSocket endpoint
  and broadcasts received events via Phoenix.PubSub for local consumption.

  ## Features
  - Auto-reconnect with exponential backoff and jitter
  - Configurable max reconnection attempts
  - Event parsing and validation
  - PubSub broadcasting for local GenServers
  - Connection state tracking
  - Graceful degradation on persistent failures
  - Event filtering support (entity_id, event_type)

  ## Topics
  - `events:all` - All events
  - `events:{entity_id}` - Events for specific entity
  - `events:type:{event_type}` - Events by type

  ## Configuration

      config :mcp_server_elixir,
        core_ws_url: "ws://localhost:3900/api/v1/events/stream",
        core_ws_enabled: true,
        core_ws_max_reconnect_attempts: 10,
        core_ws_initial_backoff_ms: 1_000,
        core_ws_max_backoff_ms: 30_000

  ## Example

      # Subscribe to all events
      Phoenix.PubSub.subscribe(McpServerElixir.PubSub, "events:all")

      # Subscribe to specific entity
      Phoenix.PubSub.subscribe(McpServerElixir.PubSub, "events:user-123")

      # Receive events
      def handle_info({:new_event, event}, state) do
        IO.inspect(event, label: "Received event")
        {:noreply, state}
      end
  """

  use WebSockex
  require Logger

  alias McpServerElixir.Infrastructure.CoreCredentials

  @default_url "ws://localhost:3900/api/v1/events/stream"
  @default_initial_backoff_ms 1_000
  @default_max_backoff_ms 30_000
  @default_max_reconnect_attempts 10
  @pubsub McpServerElixir.PubSub

  # Client API

  @doc """
  Start the WebSocket client.

  ## Options
    * `:url` - WebSocket URL (default: ws://localhost:3900/api/v1/events/stream)
    * `:name` - Process name (default: __MODULE__)
    * `:enabled` - Whether to start the client (default: true in prod, false in test)
    * `:filters` - Initial event filters (entity_id, event_type)
  """
  def start_link(opts \\ []) do
    enabled = opts[:enabled] || Application.get_env(:mcp_server_elixir, :core_ws_enabled, true)

    if enabled do
      do_start_link(opts)
    else
      Logger.info("[CoreWebSocketClient] Disabled, not connecting")
      :ignore
    end
  end

  defp do_start_link(opts) do
    config = build_config(opts)
    state = build_initial_state(config)

    Logger.info("[CoreWebSocketClient] Connecting to #{config.url}")

    connect_websocket(config.url, config.name, state, config.extra_headers)
  end

  defp build_config(opts) do
    %{
      url: opts[:url] || Application.get_env(:mcp_server_elixir, :core_ws_url, @default_url),
      name: opts[:name] || __MODULE__,
      initial_backoff:
        opts[:initial_backoff_ms] ||
          Application.get_env(
            :mcp_server_elixir,
            :core_ws_initial_backoff_ms,
            @default_initial_backoff_ms
          ),
      max_backoff:
        opts[:max_backoff_ms] ||
          Application.get_env(:mcp_server_elixir, :core_ws_max_backoff_ms, @default_max_backoff_ms),
      max_attempts:
        opts[:max_reconnect_attempts] ||
          Application.get_env(
            :mcp_server_elixir,
            :core_ws_max_reconnect_attempts,
            @default_max_reconnect_attempts
          ),
      filters: opts[:filters] || %{},
      extra_headers: build_auth_headers()
    }
  end

  defp build_initial_state(config) do
    %{
      url: config.url,
      backoff_ms: config.initial_backoff,
      initial_backoff_ms: config.initial_backoff,
      max_backoff_ms: config.max_backoff,
      max_reconnect_attempts: config.max_attempts,
      extra_headers: config.extra_headers,
      filters: config.filters,
      connected: false,
      reconnect_attempts: 0,
      total_reconnects: 0,
      events_received: 0,
      last_event_at: nil,
      last_error: nil,
      started_at: DateTime.utc_now()
    }
  end

  defp connect_websocket(url, name, state, extra_headers) do
    case WebSockex.start_link(url, __MODULE__, state, name: name, extra_headers: extra_headers) do
      {:ok, pid} ->
        {:ok, pid}

      {:error, %WebSockex.ConnError{original: reason}} ->
        Logger.warning("[CoreWebSocketClient] Failed to connect: #{inspect(reason)}, will retry")
        start_retry_loop(url, name, state)

      {:error, reason} ->
        Logger.error("[CoreWebSocketClient] Failed to start: #{inspect(reason)}")
        {:error, reason}
    end
  end

  @doc false
  # Asked again on every connect: a client assertion lives 60s, so headers
  # captured at start would be refused on a later reconnect.
  # Public (undocumented) so tests can assert the header without a live socket.
  def build_auth_headers, do: CoreCredentials.headers("Authorization")

  @doc false
  def refresh_auth(%WebSockex.Conn{extra_headers: headers} = conn) do
    kept = Enum.reject(headers, fn {name, _} -> String.downcase(name) == "authorization" end)
    %{conn | extra_headers: kept ++ build_auth_headers()}
  end

  defp start_retry_loop(url, name, initial_state) do
    Task.start_link(fn ->
      retry_connection(url, name, initial_state, 1)
    end)

    :ignore
  end

  defp retry_connection(url, name, state, attempt) when attempt <= state.max_reconnect_attempts do
    backoff = calculate_backoff(state.initial_backoff_ms * attempt, state.max_backoff_ms)

    Logger.info(
      "[CoreWebSocketClient] Retry #{attempt}/#{state.max_reconnect_attempts} in #{backoff}ms"
    )

    :telemetry.execute(
      [:mcp_server_elixir, :websocket, :reconnect_attempt],
      %{attempt: attempt, backoff_ms: backoff},
      %{url: url}
    )

    Process.sleep(backoff)

    case WebSockex.start_link(url, __MODULE__, %{state | reconnect_attempts: attempt},
           name: name,
           extra_headers: build_auth_headers()
         ) do
      {:ok, _pid} ->
        Logger.info("[CoreWebSocketClient] Connected after #{attempt} retries")

        :telemetry.execute(
          [:mcp_server_elixir, :websocket, :reconnect_success],
          %{attempts: attempt},
          %{url: url}
        )

        :ok

      {:error, reason} ->
        Logger.warning("[CoreWebSocketClient] Retry #{attempt} failed: #{inspect(reason)}")
        retry_connection(url, name, %{state | last_error: reason}, attempt + 1)
    end
  end

  defp retry_connection(url, _name, state, attempt) do
    Logger.error(
      "[CoreWebSocketClient] Failed to connect after #{attempt - 1} retries, entering degraded mode",
      url: url,
      last_error: inspect(state.last_error)
    )

    :telemetry.execute(
      [:mcp_server_elixir, :websocket, :reconnect_exhausted],
      %{attempts: attempt - 1},
      %{url: url, last_error: state.last_error}
    )

    # Don't crash - just enter degraded mode where real-time updates won't work
    # The service can still function without real-time events
    :error
  end

  defp calculate_backoff(base_ms, max_ms) do
    # Add jitter (±20%) and cap at max
    jitter = :rand.uniform() * 0.4 - 0.2
    backoff = round(base_ms * (1 + jitter))
    min(backoff, max_ms)
  end

  @doc """
  Set event filters for the connection.

  ## Parameters
    * `filters` - Map with optional `:entity_id` and `:event_type` keys

  ## Example
      CoreWebSocketClient.set_filters(%{entity_id: "user-123"})
  """
  def set_filters(pid \\ __MODULE__, filters) when is_map(filters) do
    WebSockex.cast(pid, {:set_filters, filters})
  end

  @doc """
  Get the current connection status.
  """
  def status(pid \\ __MODULE__) do
    WebSockex.cast(pid, {:get_status, self()})

    receive do
      {:status, status} -> status
    after
      5_000 -> {:error, :timeout}
    end
  end

  @doc """
  Get statistics about the WebSocket connection.
  """
  def stats(pid \\ __MODULE__) do
    WebSockex.cast(pid, {:get_stats, self()})

    receive do
      {:stats, stats} -> stats
    after
      5_000 -> {:error, :timeout}
    end
  end

  # WebSockex Callbacks

  @impl WebSockex
  def handle_connect(_conn, state) do
    Logger.info("[CoreWebSocketClient] Connected to Core WebSocket", url: state.url)

    :telemetry.execute(
      [:mcp_server_elixir, :websocket, :connected],
      %{reconnect_attempts: state.reconnect_attempts},
      %{url: state.url}
    )

    new_state = %{
      state
      | connected: true,
        backoff_ms: state.initial_backoff_ms,
        reconnect_attempts: 0,
        last_error: nil
    }

    # Send initial filters if any
    if map_size(state.filters) > 0 do
      send_filters(state.filters)
    end

    {:ok, new_state}
  end

  @impl WebSockex
  def handle_frame({:text, json}, state) do
    case Jason.decode(json) do
      {:ok, event} ->
        broadcast_event(event)

        :telemetry.execute(
          [:mcp_server_elixir, :websocket, :message_received],
          %{size_bytes: byte_size(json)},
          %{message_type: "event", event_type: event["event_type"]}
        )

        new_state = %{
          state
          | events_received: state.events_received + 1,
            last_event_at: DateTime.utc_now()
        }

        {:ok, new_state}

      {:error, reason} ->
        Logger.warning("[CoreWebSocketClient] Failed to parse event: #{inspect(reason)}",
          error: inspect(reason)
        )

        {:ok, state}
    end
  end

  @impl WebSockex
  def handle_frame({:binary, _data}, state) do
    Logger.debug("[CoreWebSocketClient] Received binary frame (ignored)")
    {:ok, state}
  end

  @impl WebSockex
  def handle_frame({:ping, _}, state) do
    {:reply, :pong, state}
  end

  @impl WebSockex
  def handle_frame({:pong, _}, state) do
    {:ok, state}
  end

  @impl WebSockex
  def handle_cast({:set_filters, filters}, state) do
    Logger.info("[CoreWebSocketClient] Setting filters: #{inspect(filters)}")
    new_state = %{state | filters: filters}

    case send_filters(filters) do
      :ok -> {:ok, new_state}
      {:error, _reason} -> {:ok, new_state}
    end
  end

  @impl WebSockex
  def handle_cast({:get_status, from}, state) do
    status =
      if state.connected do
        :connected
      else
        :disconnected
      end

    send(from, {:status, status})
    {:ok, state}
  end

  @impl WebSockex
  def handle_cast({:get_stats, from}, state) do
    stats = %{
      connected: state.connected,
      reconnect_attempts: state.reconnect_attempts,
      total_reconnects: state.total_reconnects,
      max_reconnect_attempts: state.max_reconnect_attempts,
      events_received: state.events_received,
      last_event_at: state.last_event_at,
      last_error: state.last_error,
      started_at: state.started_at,
      url: state.url,
      filters: state.filters
    }

    send(from, {:stats, stats})
    {:ok, state}
  end

  @impl WebSockex
  def handle_disconnect(%{reason: reason} = status, state) do
    new_attempts = state.reconnect_attempts + 1
    new_total = state.total_reconnects + 1

    Logger.warning(
      "[CoreWebSocketClient] Disconnected: #{inspect(reason)} (attempt #{new_attempts}/#{state.max_reconnect_attempts})",
      reason: inspect(reason),
      reconnect_attempts: new_attempts
    )

    :telemetry.execute(
      [:mcp_server_elixir, :websocket, :disconnected],
      %{reconnect_attempts: new_attempts},
      %{url: state.url, reason: reason}
    )

    # Check if we've exceeded max reconnection attempts
    if new_attempts > state.max_reconnect_attempts do
      Logger.error(
        "[CoreWebSocketClient] Max reconnection attempts (#{state.max_reconnect_attempts}) exceeded, stopping",
        url: state.url,
        total_reconnects: new_total
      )

      :telemetry.execute(
        [:mcp_server_elixir, :websocket, :reconnect_exhausted],
        %{attempts: new_attempts, total_reconnects: new_total},
        %{url: state.url}
      )

      # Stop reconnecting - the supervision tree will handle restart if configured
      {:ok, %{state | connected: false, last_error: reason}}
    else
      new_state = %{
        state
        | connected: false,
          reconnect_attempts: new_attempts,
          total_reconnects: new_total,
          last_error: reason
      }

      # Exponential backoff with jitter
      backoff = calculate_backoff(new_state.backoff_ms, state.max_backoff_ms)

      Logger.info(
        "[CoreWebSocketClient] Reconnecting in #{backoff}ms (attempt #{new_attempts}/#{state.max_reconnect_attempts})"
      )

      Process.sleep(backoff)

      reconnect(status, %{new_state | backoff_ms: min(backoff * 2, state.max_backoff_ms)})
    end
  end

  @impl WebSockex
  def terminate(reason, _state) do
    Logger.info("[CoreWebSocketClient] Terminating: #{inspect(reason)}")
    :ok
  end

  # Private Functions

  defp reconnect(%{conn: %WebSockex.Conn{} = conn}, state),
    do: {:reconnect, refresh_auth(conn), state}

  defp reconnect(_status, state), do: {:reconnect, state}

  defp send_filters(filters) do
    filter_json = Jason.encode!(filters)
    WebSockex.send_frame(self(), {:text, filter_json})
  rescue
    error ->
      Logger.warning("[CoreWebSocketClient] Failed to send filters: #{inspect(error)}")
      {:error, error}
  end

  defp broadcast_event(event) do
    # Broadcast to all events topic
    Phoenix.PubSub.broadcast(@pubsub, "events:all", {:new_event, event})

    # Broadcast to entity-specific topic
    if entity_id = event["entity_id"] do
      Phoenix.PubSub.broadcast(@pubsub, "events:#{entity_id}", {:new_event, event})
    end

    # Broadcast to event-type topic
    if event_type = event["event_type"] do
      Phoenix.PubSub.broadcast(@pubsub, "events:type:#{event_type}", {:new_event, event})
    end

    Logger.debug("[CoreWebSocketClient] Broadcast event: #{event["id"] || "unknown"}")
  end
end
