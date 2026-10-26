defmodule QueryServiceExWeb.ProjectionChannelTest do
  use QueryServiceExWeb.ChannelCase

  alias QueryServiceExWeb.ProjectionChannel

  @tenant "tenant-test-1"

  setup do
    {:ok, socket, user, _token} = create_authenticated_socket(tenant_id: @tenant)
    {:ok, socket: socket, user: user}
  end

  describe "join projections:{name}" do
    test "successfully joins projection channel", %{socket: socket} do
      {:ok, reply, _socket} =
        subscribe_and_join(socket, ProjectionChannel, "projections:user_stats")

      assert reply.status == "subscribed"
      assert reply.projection == "user_stats"
    end

    test "assigns projection_name to socket", %{socket: socket} do
      {:ok, _reply, socket} =
        subscribe_and_join(socket, ProjectionChannel, "projections:order_totals")

      assert socket.assigns.projection_name == "order_totals"
    end

    test "receives presence_state after joining", %{socket: socket} do
      {:ok, _reply, _socket} =
        subscribe_and_join(socket, ProjectionChannel, "projections:user_stats")

      assert_push("presence_state", _state, 1_000)
    end
  end

  describe "state_updated broadcasts" do
    test "receives state_updated messages", %{socket: socket} do
      {:ok, _reply, _socket} =
        subscribe_and_join(socket, ProjectionChannel, "projections:user_stats")

      state = %{"event_count" => 42, "last_updated" => "2026-02-02T12:00:00Z"}

      broadcast_projection_update(@tenant, "user_stats", "user-123", state, version: 5)

      assert_push("state_updated", update, 1_000)
      assert update.entity_id == "user-123"
      assert update.state == state
      assert update.version == 5
    end

    test "does not receive updates for other projections", %{socket: socket} do
      {:ok, _reply, _socket} =
        subscribe_and_join(socket, ProjectionChannel, "projections:user_stats")

      # Broadcast to different projection
      broadcast_projection_update(@tenant, "order_totals", "order-123", %{"total" => 500})

      refute_push("state_updated", _)
    end
  end

  describe "projection_error broadcasts" do
    test "receives projection_error messages", %{socket: socket} do
      {:ok, _reply, _socket} =
        subscribe_and_join(socket, ProjectionChannel, "projections:user_stats")

      broadcast_projection_error(@tenant, "user_stats", "user-456", "Failed to process event",
        event_id: "evt-789"
      )

      assert_push("projection_error", error, 1_000)
      assert error.entity_id == "user-456"
      assert error.error == "Failed to process event"
      assert error.event_id == "evt-789"
    end
  end

  describe "get_state incoming message" do
    test "returns error for missing entity_id", %{socket: socket} do
      {:ok, _reply, socket} =
        subscribe_and_join(socket, ProjectionChannel, "projections:user_stats")

      ref = push(socket, "get_state", %{})
      assert_reply(ref, :error, %{reason: "missing_entity_id"}, 1_000)
    end

    test "returns error for non-existent state", %{socket: socket} do
      {:ok, _reply, socket} =
        subscribe_and_join(socket, ProjectionChannel, "projections:nonexistent_projection")

      ref = push(socket, "get_state", %{"entity_id" => "user-999"})
      assert_reply(ref, :error, %{reason: "not_found", entity_id: "user-999"}, 1_000)
    end

    test "returns the tenant's folded projection state", %{socket: socket} do
      projection = "channel_state_#{System.unique_integer([:positive])}"
      put_folded(@tenant, projection, "user-1", %{"count" => 3})

      {:ok, _reply, socket} =
        subscribe_and_join(socket, ProjectionChannel, "projections:" <> projection)

      ref = push(socket, "get_state", %{"entity_id" => "user-1"})
      assert_reply(ref, :ok, %{"count" => 3}, 1_000)
    end

    test "never returns another tenant's state", %{socket: socket} do
      projection = "channel_state_#{System.unique_integer([:positive])}"
      put_folded("other-tenant", projection, "user-1", %{"count" => 9})

      {:ok, _reply, socket} =
        subscribe_and_join(socket, ProjectionChannel, "projections:" <> projection)

      ref = push(socket, "get_state", %{"entity_id" => "user-1"})
      assert_reply(ref, :error, %{reason: "not_found"}, 1_000)
    end
  end

  defp put_folded(tenant, projection, entity_id, state) do
    generation = make_ref()
    :ets.insert(:tenant_projection_generation, {{tenant, projection}, generation})
    :ets.insert(:tenant_projection_state, {{tenant, projection, generation, entity_id}, state})

    on_exit(fn ->
      :ets.delete(:tenant_projection_generation, {tenant, projection})
      :ets.delete(:tenant_projection_state, {tenant, projection, generation, entity_id})
    end)
  end

  describe "presence tracking" do
    test "tracks projection name in presence metadata", %{socket: socket, user: user} do
      {:ok, _reply, _socket} =
        subscribe_and_join(socket, ProjectionChannel, "projections:user_stats")

      assert_push("presence_state", state, 1_000)

      user_id_str = to_string(user.id)
      assert Map.has_key?(state, user_id_str)

      metas = state[user_id_str].metas
      assert metas != []

      [meta | _] = metas
      assert meta.projection == "user_stats"
    end
  end
end
