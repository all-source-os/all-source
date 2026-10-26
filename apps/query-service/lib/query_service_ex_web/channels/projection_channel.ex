defmodule QueryServiceExWeb.ProjectionChannel do
  @moduledoc """
  Phoenix Channel for real-time projection state updates.

  Allows clients to subscribe to projection state changes and receive updates
  as projections process new events.

  ## Topics

  - `projections:{name}` - Subscribe to updates for a specific projection (e.g., `projections:user_stats`)

  ## Messages

  ### Outgoing

  - `state_updated` - Pushed when projection state changes:

        channel.on("state_updated", update => {
          console.log(update);
          // {
          //   entity_id: "user-123",
          //   state: {...},
          //   version: 42,
          //   updated_at: "2026-02-02T12:00:00Z"
          // }
        });

  - `projection_error` - Pushed when projection encounters an error:

        channel.on("projection_error", error => {
          console.log(error);
          // {
          //   entity_id: "user-123",
          //   error: "Failed to process event",
          //   event_id: "..."
          // }
        });

  ### Incoming

  - `get_state` - Request current state for an entity:

        channel.push("get_state", {entity_id: "user-123"})
          .receive("ok", state => console.log(state))
          .receive("error", err => console.log(err));

  ## Presence

  This channel supports Phoenix Presence for tracking connected clients.
  """

  use Phoenix.Channel
  require Logger

  alias Phoenix.PubSub
  alias QueryServiceEx.Projections.TenantProjections
  alias QueryServiceExWeb.Presence

  # Intercept presence_diff to handle via handle_out/3
  intercept(["presence_diff"])

  @pubsub QueryServiceEx.PubSub

  @impl true
  def join("projections:" <> projection_name, _payload, socket) do
    # Subscribe to the TENANT-SCOPED projection topic so a client only receives
    # its own tenant's projection updates. Fail closed: no tenant → reject.
    case socket.assigns[:tenant_id] do
      tenant when is_binary(tenant) and tenant != "" ->
        send(self(), :after_join)
        PubSub.subscribe(@pubsub, "projections:#{tenant}:#{projection_name}")

        Logger.info("[ProjectionChannel] User joined projections:#{projection_name}",
          user_id: socket.assigns.user_id,
          tenant_id: tenant,
          projection_name: projection_name
        )

        emit_channel_joined("projections")
        socket = assign(socket, :projection_name, projection_name)

        {:ok, %{status: "subscribed", projection: projection_name}, socket}

      _ ->
        {:error, %{reason: "unauthorized: no tenant on socket"}}
    end
  end

  @impl true
  def handle_info(:after_join, socket) do
    {:ok, _} =
      Presence.track(socket, socket.assigns.user_id, %{
        user_id: socket.assigns.user_id,
        tenant_id: socket.assigns.tenant_id,
        projection: socket.assigns[:projection_name],
        online_at: System.system_time(:second)
      })

    QueryServiceExWeb.FundedChannel.deliver(socket, "presence_state", Presence.list(socket))
  end

  # Handle projection state updates from PubSub
  def handle_info({:state_updated, update}, socket) do
    QueryServiceExWeb.FundedChannel.deliver(socket, "state_updated", update)
  end

  def handle_info({:projection_error, error}, socket) do
    QueryServiceExWeb.FundedChannel.deliver(socket, "projection_error", error)
  end

  @impl true
  def handle_in("get_state", %{"entity_id" => entity_id}, socket) do
    case get_projection_state(socket.assigns, entity_id) do
      {:ok, state} ->
        QueryServiceExWeb.FundedChannel.reply(socket, {:ok, state})

      {:error, :not_found} ->
        QueryServiceExWeb.FundedChannel.reply(
          socket,
          {:error, %{reason: "not_found", entity_id: entity_id}}
        )
    end
  end

  def handle_in("get_state", _payload, socket) do
    QueryServiceExWeb.FundedChannel.reply(socket, {:error, %{reason: "missing_entity_id"}})
  end

  # Handle presence_diff broadcasts from Presence tracker
  @impl true
  def handle_out("presence_diff", msg, socket) do
    QueryServiceExWeb.FundedChannel.deliver(socket, "presence_diff", msg)
  end

  @impl true
  def terminate(reason, socket) do
    Logger.info("[ProjectionChannel] User left",
      user_id: socket.assigns.user_id,
      projection: socket.assigns[:projection_name],
      reason: inspect(reason)
    )

    emit_channel_left("projections")
    :ok
  end

  # Telemetry emissions for channel metrics
  defp emit_channel_joined(channel) do
    :telemetry.execute(
      [:query_service_ex, :channel, :joined],
      %{count: 1},
      %{channel: channel}
    )
  end

  defp emit_channel_left(channel) do
    :telemetry.execute(
      [:query_service_ex, :channel, :left],
      %{count: 1},
      %{channel: channel}
    )
  end

  # Private functions

  defp get_projection_state(%{tenant_id: tenant, projection_name: projection}, entity_id)
       when is_binary(tenant) and is_binary(projection) and is_binary(entity_id) do
    TenantProjections.get_state(tenant, projection, entity_id)
  end

  defp get_projection_state(_assigns, _entity_id), do: {:error, :not_found}
end
