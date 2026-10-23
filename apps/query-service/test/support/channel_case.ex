defmodule QueryServiceExWeb.ChannelCase do
  @moduledoc """
  This module defines the test case to be used by channel tests.
  """

  use ExUnit.CaseTemplate

  using do
    quote do
      import Phoenix.ChannelTest
      import QueryServiceExWeb.ChannelCase

      alias QueryServiceEx.AuthHelpers

      @endpoint QueryServiceExWeb.Endpoint

      @doc """
      Creates an authenticated socket with a test user.

      Returns `{:ok, socket, user, token}` where:
      - `socket` is a connected Phoenix.Socket
      - `user` is the user map
      - `token` is the JWT token
      """
      def create_authenticated_socket(opts \\ []) do
        user_id = Keyword.get(opts, :user_id, "user-#{:rand.uniform(100_000)}")
        tenant_id = Keyword.get(opts, :tenant_id, "tenant-#{:rand.uniform(100_000)}")
        email = Keyword.get(opts, :email, "test#{:rand.uniform(100_000)}@example.com")

        user = %{
          id: user_id,
          email: email,
          name: "Test User",
          tenant_id: tenant_id,
          role: "user"
        }

        token = AuthHelpers.encode_jwt(user)
        {:ok, socket} = connect(QueryServiceExWeb.UserSocket, %{"token" => token})
        {:ok, socket, user, token}
      end

      def create_unauthenticated_socket do
        connect(QueryServiceExWeb.UserSocket, %{"token" => "invalid_token"})
      end

      def create_socket_without_token do
        connect(QueryServiceExWeb.UserSocket, %{})
      end
    end
  end

  setup _tags do
    QueryServiceEx.ChannelEgressFixture.start()
    System.put_env("JWT_SECRET", QueryServiceEx.AuthHelpers.test_jwt_secret())
    on_exit(fn -> System.delete_env("JWT_SECRET") end)
    :ok
  end

  # Broadcast helpers for simulating Core WebSocket events in tests. Topics are
  # tenant-scoped (events:<tenant>:...) — the event must carry a tenant_id.
  def broadcast_event(event) do
    case event["tenant_id"] || event[:tenant_id] do
      nil ->
        :ok

      tenant ->
        Phoenix.PubSub.broadcast(
          QueryServiceEx.PubSub,
          "events:#{tenant}:all",
          {:new_event, event}
        )

        entity_id = event["entity_id"] || event[:entity_id]

        if entity_id do
          Phoenix.PubSub.broadcast(
            QueryServiceEx.PubSub,
            "events:#{tenant}:#{entity_id}",
            {:new_event, event}
          )
        end

        event_type = event["event_type"] || event[:event_type]

        if event_type do
          Phoenix.PubSub.broadcast(
            QueryServiceEx.PubSub,
            "events:#{tenant}:type:#{event_type}",
            {:new_event, event}
          )
        end

        :ok
    end
  end

  def broadcast_projection_update(tenant_id, projection_name, entity_id, state, opts \\ []) do
    update = %{
      entity_id: entity_id,
      state: state,
      version: Keyword.get(opts, :version, 1),
      updated_at: Keyword.get(opts, :updated_at, DateTime.utc_now() |> DateTime.to_iso8601())
    }

    Phoenix.PubSub.broadcast(
      QueryServiceEx.PubSub,
      "projections:#{tenant_id}:#{projection_name}",
      {:state_updated, update}
    )

    :ok
  end

  def broadcast_projection_error(tenant_id, projection_name, entity_id, error, opts \\ []) do
    error_msg = %{
      entity_id: entity_id,
      error: error,
      event_id: Keyword.get(opts, :event_id),
      occurred_at: DateTime.utc_now() |> DateTime.to_iso8601()
    }

    Phoenix.PubSub.broadcast(
      QueryServiceEx.PubSub,
      "projections:#{tenant_id}:#{projection_name}",
      {:projection_error, error_msg}
    )

    :ok
  end
end
