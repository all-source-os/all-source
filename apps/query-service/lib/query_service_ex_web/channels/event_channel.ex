defmodule QueryServiceExWeb.EventChannel do
  @moduledoc """
  Phoenix Channel for real-time event streaming.

  Bridges internal PubSub events from CoreWebSocketClient to external WebSocket clients.

  ## Topics

  - `events:all` - Subscribe to all events
  - `events:{entity_id}` - Subscribe to events for a specific entity (e.g., `events:user-123`)
  - `events:type:{event_type}` - Subscribe to events of a specific type (e.g., `events:type:user.created`)

  ## Messages

  Incoming events are pushed as `new_event` messages:

      channel.on("new_event", event => {
        console.log(event);
        // {
        //   id: "...",
        //   entity_id: "user-123",
        //   event_type: "user.created",
        //   payload: {...},
        //   timestamp: "2026-02-02T12:00:00Z"
        // }
      });

  ## Presence

  This channel supports Phoenix Presence for tracking connected clients:

      channel.on("presence_state", state => {
        console.log("Current users:", state);
      });

      channel.on("presence_diff", diff => {
        console.log("User joined/left:", diff);
      });
  """

  use Phoenix.Channel
  require Logger

  alias Phoenix.PubSub
  alias QueryServiceExWeb.Presence

  # Intercept presence_diff to handle via handle_out/3
  intercept(["presence_diff"])

  @pubsub QueryServiceEx.PubSub

  # All joins subscribe to a TENANT-SCOPED internal PubSub topic derived from the
  # authenticated socket's tenant. The client-facing topic name is unchanged
  # (`events:all`, `events:<entity>`, `events:type:<type>`), but a subscriber
  # only ever receives its own tenant's events — there is no global topic a user
  # can join to see another tenant's data. Fail closed: no tenant on the socket
  # → join rejected.
  @impl true
  def join("events:all", _payload, socket) do
    with {:ok, tenant} <- tenant_scope(socket) do
      send(self(), :after_join)
      PubSub.subscribe(@pubsub, "events:#{tenant}:all")

      Logger.info("[EventChannel] User joined events:all",
        user_id: socket.assigns.user_id,
        tenant_id: tenant
      )

      emit_channel_joined("events:all")
      {:ok, %{status: "subscribed", topic: "events:all"}, socket}
    end
  end

  def join("events:type:" <> event_type, _payload, socket) do
    with {:ok, tenant} <- tenant_scope(socket) do
      send(self(), :after_join)
      PubSub.subscribe(@pubsub, "events:#{tenant}:type:#{event_type}")

      Logger.info("[EventChannel] User joined events:type:#{event_type}",
        user_id: socket.assigns.user_id,
        tenant_id: tenant,
        event_type: event_type
      )

      emit_channel_joined("events:type:#{event_type}")

      {:ok, %{status: "subscribed", topic: "events:type:#{event_type}", event_type: event_type},
       socket}
    end
  end

  def join("events:" <> entity_id, _payload, socket) do
    with {:ok, tenant} <- tenant_scope(socket) do
      send(self(), :after_join)
      PubSub.subscribe(@pubsub, "events:#{tenant}:#{entity_id}")

      Logger.info("[EventChannel] User joined events:#{entity_id}",
        user_id: socket.assigns.user_id,
        tenant_id: tenant,
        entity_id: entity_id
      )

      emit_channel_joined("events:#{entity_id}")
      {:ok, %{status: "subscribed", topic: "events:#{entity_id}", entity_id: entity_id}, socket}
    end
  end

  # Resolve the socket's tenant, rejecting the join when absent (fail closed).
  # Requires a non-empty BINARY — a JWT with a null tenant decodes to the atom
  # `:null`, which must NOT be accepted (it would build an `events::null:...`
  # topic). Anything that isn't a real tenant string is rejected.
  defp tenant_scope(socket) do
    case socket.assigns[:tenant_id] do
      tenant when is_binary(tenant) and tenant != "" -> {:ok, tenant}
      _ -> {:error, %{reason: "unauthorized: no tenant on socket"}}
    end
  end

  @impl true
  def handle_info(:after_join, socket) do
    {:ok, _} =
      Presence.track(socket, socket.assigns.user_id, %{
        user_id: socket.assigns.user_id,
        tenant_id: socket.assigns.tenant_id,
        online_at: System.system_time(:second)
      })

    QueryServiceExWeb.FundedChannel.deliver(socket, "presence_state", Presence.list(socket))
  end

  # Handle PubSub events from CoreWebSocketClient. The subscription is already
  # tenant-scoped; this strict tenant-match is defense in depth — an event is
  # pushed ONLY when its tenant equals the socket's tenant, so a broadcast bug
  # can never spill another tenant's (or the `system` tenant's) events.
  def handle_info({:new_event, event}, socket) do
    if event_tenant(event) == socket.assigns[:tenant_id] do
      QueryServiceExWeb.FundedChannel.deliver(socket, "new_event", event)
    else
      {:noreply, socket}
    end
  end

  defp event_tenant(event), do: event["tenant_id"] || event[:tenant_id]

  # Handle presence_diff broadcasts from Presence tracker
  @impl true
  def handle_out("presence_diff", msg, socket) do
    QueryServiceExWeb.FundedChannel.deliver(socket, "presence_diff", msg)
  end

  @impl true
  def terminate(reason, socket) do
    Logger.info("[EventChannel] User left",
      user_id: socket.assigns.user_id,
      reason: inspect(reason)
    )

    emit_channel_left(socket.topic)
    :ok
  end

  # Telemetry emissions for channel metrics
  defp emit_channel_joined(channel) do
    :telemetry.execute(
      [:query_service_ex, :channel, :joined],
      %{count: 1},
      %{channel: normalize_channel(channel)}
    )
  end

  defp emit_channel_left(channel) do
    :telemetry.execute(
      [:query_service_ex, :channel, :left],
      %{count: 1},
      %{channel: normalize_channel(channel)}
    )
  end

  # Normalize channel names for metrics (avoid high cardinality)
  defp normalize_channel("events:all"), do: "events:all"
  defp normalize_channel("events:type:" <> _), do: "events:type:*"
  defp normalize_channel("events:" <> _), do: "events:entity:*"
  defp normalize_channel(other), do: other
end
