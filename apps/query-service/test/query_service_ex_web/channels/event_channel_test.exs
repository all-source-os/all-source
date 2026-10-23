defmodule QueryServiceExWeb.EventChannelTest do
  use QueryServiceExWeb.ChannelCase

  alias QueryServiceExWeb.EventChannel

  @tenant "tenant-test-1"
  @other "tenant-test-2"

  setup do
    {:ok, socket, user, _token} = create_authenticated_socket(tenant_id: @tenant)
    {:ok, socket: socket, user: user}
  end

  describe "join events:all" do
    test "successfully joins events:all channel", %{socket: socket} do
      {:ok, reply, _socket} = subscribe_and_join(socket, EventChannel, "events:all")

      assert reply.status == "subscribed"
      assert reply.topic == "events:all"
    end

    test "receives events after joining events:all", %{socket: socket} do
      {:ok, _reply, _socket} = subscribe_and_join(socket, EventChannel, "events:all")

      event = %{
        "id" => "event-#{:rand.uniform(100_000)}",
        "tenant_id" => @tenant,
        "entity_id" => "user-123",
        "event_type" => "user.created",
        "payload" => %{"name" => "John"},
        "timestamp" => DateTime.utc_now() |> DateTime.to_iso8601()
      }

      broadcast_event(event)

      assert_push("new_event", pushed_event, 1_000)
      assert pushed_event["id"] == event["id"]
      assert pushed_event["entity_id"] == "user-123"
    end

    test "receives presence_state after joining", %{socket: socket} do
      {:ok, _reply, _socket} = subscribe_and_join(socket, EventChannel, "events:all")

      assert_push("presence_state", _state, 1_000)
    end

    test "rejects join when the socket has no tenant" do
      {:ok, socket, _user, _token} = create_authenticated_socket(tenant_id: nil)

      assert {:error, %{reason: reason}} =
               subscribe_and_join(socket, EventChannel, "events:all")

      assert reason =~ "no tenant"
    end

    # --- tenant-isolation gate ---
    test "does NOT receive another tenant's event", %{socket: socket} do
      {:ok, _reply, _socket} = subscribe_and_join(socket, EventChannel, "events:all")

      foreign_event = %{
        "id" => "event-foreign",
        "tenant_id" => @other,
        "entity_id" => "user-123",
        "event_type" => "user.created",
        "payload" => %{}
      }

      broadcast_event(foreign_event)
      refute_push("new_event", _, 50)
    end
  end

  describe "join events:{entity_id}" do
    test "successfully joins entity-specific channel", %{socket: socket} do
      {:ok, reply, _socket} = subscribe_and_join(socket, EventChannel, "events:user-456")

      assert reply.status == "subscribed"
      assert reply.topic == "events:user-456"
      assert reply.entity_id == "user-456"
    end

    test "receives only events for subscribed entity", %{socket: socket} do
      {:ok, _reply, _socket} = subscribe_and_join(socket, EventChannel, "events:user-456")

      non_matching_event = %{
        "id" => "event-2",
        "tenant_id" => @tenant,
        "entity_id" => "user-789",
        "event_type" => "user.updated",
        "payload" => %{}
      }

      broadcast_event(non_matching_event)
      refute_push("new_event", _, 50)

      matching_event = %{
        "id" => "event-1",
        "tenant_id" => @tenant,
        "entity_id" => "user-456",
        "event_type" => "user.updated",
        "payload" => %{}
      }

      broadcast_event(matching_event)

      assert_push("new_event", pushed_event, 1_000)
      assert pushed_event["entity_id"] == "user-456"
    end
  end

  describe "join events:type:{event_type}" do
    test "successfully joins event type channel", %{socket: socket} do
      {:ok, reply, _socket} = subscribe_and_join(socket, EventChannel, "events:type:order.placed")

      assert reply.status == "subscribed"
      assert reply.topic == "events:type:order.placed"
      assert reply.event_type == "order.placed"
    end

    test "receives only events of subscribed type", %{socket: socket} do
      {:ok, _reply, _socket} =
        subscribe_and_join(socket, EventChannel, "events:type:order.placed")

      non_matching_event = %{
        "id" => "event-2",
        "tenant_id" => @tenant,
        "entity_id" => "order-456",
        "event_type" => "order.shipped",
        "payload" => %{}
      }

      broadcast_event(non_matching_event)
      refute_push("new_event", _, 50)

      matching_event = %{
        "id" => "event-1",
        "tenant_id" => @tenant,
        "entity_id" => "order-123",
        "event_type" => "order.placed",
        "payload" => %{"total" => 100}
      }

      broadcast_event(matching_event)

      assert_push("new_event", pushed_event, 1_000)
      assert pushed_event["event_type"] == "order.placed"
    end
  end

  describe "multiple subscriptions" do
    test "can join multiple event channels", %{socket: socket} do
      {:ok, _, socket} = subscribe_and_join(socket, EventChannel, "events:all")
      {:ok, _, _socket} = subscribe_and_join(socket, EventChannel, "events:user-123")

      event = %{
        "id" => "event-1",
        "tenant_id" => @tenant,
        "entity_id" => "user-123",
        "event_type" => "user.created",
        "payload" => %{}
      }

      broadcast_event(event)

      assert_push("new_event", _, 1_000)
    end
  end

  describe "presence tracking" do
    test "tracks user presence on join", %{socket: socket, user: user} do
      {:ok, _reply, _socket} = subscribe_and_join(socket, EventChannel, "events:all")

      assert_push("presence_state", state, 1_000)

      assert Map.has_key?(state, to_string(user.id))
      user_presence = state[to_string(user.id)]
      assert user_presence.metas != []
    end
  end
end
