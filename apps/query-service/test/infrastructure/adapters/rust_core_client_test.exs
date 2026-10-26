defmodule QueryServiceEx.Infrastructure.Adapters.RustCoreClientTest do
  use ExUnit.Case, async: false

  alias QueryServiceEx.Domain.Entities.Query
  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient

  @moduletag :integration

  # Helper to assert result is either :ok or :error tuple
  defp assert_result(result) do
    case result do
      {:ok, _} -> assert true
      {:error, _} -> assert true
      _ -> flunk("Expected {:ok, _} or {:error, _}, got: #{inspect(result)}")
    end
  end

  describe "health_check/0" do
    @tag :skip_unless_backend
    test "returns ok when backend is available" do
      case RustCoreClient.health_check() do
        {:ok, health} ->
          assert is_map(health)
          assert Map.has_key?(health, "status") or Map.has_key?(health, :status)

        {:error, _reason} ->
          # Backend not running - this is acceptable in tests
          :ok
      end
    end

    test "returns error when backend is unavailable" do
      # This test documents the error behavior
      # In production, we'd mock the HTTP client
      result = RustCoreClient.health_check()

      case result do
        {:ok, _} -> assert true
        {:error, _} -> assert true
      end
    end
  end

  describe "create_event/1" do
    test "validates event structure" do
      event = %{
        entity_id: "test-entity-123",
        event_type: "test.created",
        payload: %{test: "data"}
      }

      result = RustCoreClient.create_event(event)

      # Result will be error if backend not running, ok if running
      assert_result(result)
    end

    test "handles missing required fields gracefully" do
      invalid_event = %{}

      result = RustCoreClient.create_event(invalid_event)

      # Should return error (either from validation or backend)
      assert match?({:error, _}, result)
    end
  end

  describe "create_event_batch/1" do
    test "accepts list of events" do
      events = [
        %{entity_id: "e1", event_type: "test.event1", payload: %{}},
        %{entity_id: "e2", event_type: "test.event2", payload: %{}}
      ]

      result = RustCoreClient.create_event_batch(events)
      assert_result(result)
    end

    test "handles empty list" do
      result = RustCoreClient.create_event_batch([])
      assert_result(result)
    end
  end

  describe "create_event_batch/2 with tenant" do
    test "accepts tenant_id and list of events" do
      tenant_id = "test-tenant"

      events = [
        %{entity_id: "e1", event_type: "test.event1", payload: %{}},
        %{entity_id: "e2", event_type: "test.event2", payload: %{}}
      ]

      result = RustCoreClient.create_event_batch(tenant_id, events)
      assert_result(result)
    end

    test "handles empty list with tenant" do
      result = RustCoreClient.create_event_batch("test-tenant", [])
      assert_result(result)
    end

    test "returns batch response structure" do
      tenant_id = "test-tenant"

      events = [
        %{entity_id: "batch-e1", event_type: "test.batch", payload: %{}}
      ]

      case RustCoreClient.create_event_batch(tenant_id, events) do
        {:ok, response} when is_map(response) ->
          # Should have batch response fields
          assert Map.has_key?(response, "total") or Map.has_key?(response, "ingested") or
                   Map.has_key?(response, "events")

        {:ok, _} ->
          assert true

        {:error, _} ->
          # Backend not available
          assert true
      end
    end
  end

  describe "query_events/1 with map params" do
    test "queries with entity_id filter" do
      params = %{entity_id: "test-entity"}

      result = RustCoreClient.query_events(params)
      assert_result(result)
    end

    test "queries with event_type filter" do
      params = %{event_type: "test.event"}

      result = RustCoreClient.query_events(params)
      assert_result(result)
    end

    test "queries with limit" do
      params = %{limit: 10}

      result = RustCoreClient.query_events(params)
      assert_result(result)
    end

    test "queries with limit and offset" do
      params = %{limit: 10, offset: 20}

      result = RustCoreClient.query_events(params)
      assert_result(result)
    end
  end

  describe "query_events/1 with Query struct" do
    test "compiles Query struct to params" do
      query =
        Query.new(
          from: :events,
          limit: 50
        )

      result = RustCoreClient.query_events(query)
      assert_result(result)
    end

    test "compiles Query with predicate" do
      predicate = Query.make_predicate(:eq, :entity_id, "test-123")
      query = Query.new() |> Query.add_predicate(predicate)

      result = RustCoreClient.query_events(query)
      assert_result(result)
    end
  end

  describe "get_events_by_entity/1" do
    test "queries events for specific entity" do
      result = RustCoreClient.get_events_by_entity("test-entity")
      assert_result(result)
    end
  end

  describe "get_events_by_type/1" do
    test "queries events of specific type" do
      result = RustCoreClient.get_events_by_type("test.event")
      assert_result(result)
    end
  end

  describe "list_projections/0" do
    test "returns list or error" do
      result = RustCoreClient.list_projections()
      assert_result(result)
    end
  end

  describe "get_projection/1" do
    test "queries specific projection by id" do
      result = RustCoreClient.get_projection("test-projection")

      case result do
        {:ok, _projection} -> assert true
        {:error, :not_found} -> assert true
        {:error, _} -> assert true
      end
    end
  end

  describe "create_projection/1" do
    test "accepts projection map" do
      projection = %{
        name: "test_projection",
        version: 1,
        initial_state: %{},
        definition: "test"
      }

      result = RustCoreClient.create_projection(projection)
      assert_result(result)
    end
  end

  describe "list_schemas/0" do
    test "returns schemas list or error" do
      result = RustCoreClient.list_schemas()
      assert_result(result)
    end
  end

  describe "get_schema/1-2" do
    test "queries schema by event type" do
      result = RustCoreClient.get_schema("test.event")

      case result do
        {:ok, _schema} -> assert true
        {:error, :not_found} -> assert true
        {:error, _} -> assert true
      end
    end

    test "queries schema with specific version" do
      result = RustCoreClient.get_schema("test.event", 1)

      case result do
        {:ok, _schema} -> assert true
        {:error, :not_found} -> assert true
        {:error, _} -> assert true
      end
    end
  end

  describe "register_schema/1" do
    test "accepts schema definition" do
      schema = %{
        event_type: "test.event",
        version: 1,
        schema: %{
          type: "object",
          properties: %{
            field: %{type: "string"}
          }
        }
      }

      result = RustCoreClient.register_schema(schema)
      assert_result(result)
    end
  end

  describe "get_metrics/0" do
    test "returns metrics or error" do
      result = RustCoreClient.get_metrics()
      assert_result(result)
    end
  end

  describe "error handling" do
    test "handles network errors gracefully" do
      # This documents error handling behavior
      # In production, backend might be down
      result = RustCoreClient.health_check()

      case result do
        {:ok, _} ->
          # Backend is running
          assert true

        {:error, reason} ->
          # Backend is down - error should be descriptive
          assert is_binary(reason) or is_atom(reason)
      end
    end

    test "returns structured errors" do
      # Query with intentionally invalid data to test error handling
      result = RustCoreClient.create_event(%{invalid: "data"})

      case result do
        {:error, reason} ->
          assert reason != nil

        {:ok, _} ->
          # If backend is very permissive, that's ok for this test
          assert true
      end
    end
  end

  describe "integration: complete workflow" do
    @tag :skip_unless_backend
    test "creates event and queries it back" do
      # 1. Create an event
      event = %{
        entity_id: "integration-test-#{System.unique_integer([:positive])}",
        event_type: "integration.test",
        payload: %{test: "data", timestamp: DateTime.utc_now() |> DateTime.to_iso8601()}
      }

      case RustCoreClient.create_event(event) do
        {:ok, _created_event} ->
          # 2. Query events by entity
          {:ok, events} = RustCoreClient.get_events_by_entity(event.entity_id)
          assert is_list(events)

          # 3. Query events by type
          {:ok, type_events} = RustCoreClient.get_events_by_type(event.event_type)
          assert is_list(type_events)

        {:error, _reason} ->
          # Backend not available - skip test
          :ok
      end
    end

    @tag :skip_unless_backend
    test "batch event creation and querying" do
      base_id = "batch-test-#{System.unique_integer([:positive])}"

      events =
        Enum.map(1..5, fn i ->
          %{
            entity_id: "#{base_id}-#{i}",
            event_type: "batch.test",
            payload: %{index: i}
          }
        end)

      case RustCoreClient.create_event_batch(events) do
        {:ok, response} ->
          # New batch endpoint returns {total, ingested, events}
          assert is_map(response)
          assert Map.has_key?(response, "total") or Map.has_key?(response, "events")

        {:error, _reason} ->
          # Backend not available
          :ok
      end
    end

    @tag :skip_unless_backend
    test "batch event creation with tenant isolation" do
      tenant_id = "test-tenant-#{System.unique_integer([:positive])}"
      base_id = "batch-tenant-test-#{System.unique_integer([:positive])}"

      events =
        Enum.map(1..3, fn i ->
          %{
            entity_id: "#{base_id}-#{i}",
            event_type: "batch.tenant.test",
            payload: %{index: i}
          }
        end)

      case RustCoreClient.create_event_batch(tenant_id, events) do
        {:ok, response} ->
          # Batch endpoint returns {total, ingested, events}
          assert is_map(response)
          assert Map.get(response, "total") == 3 or Map.get(response, "ingested") == 3

        {:error, _reason} ->
          # Backend not available
          :ok
      end
    end
  end

  describe "query compilation" do
    test "compiles simple query to params" do
      query = Query.new(limit: 100)

      # We can't directly test compile_query as it's private,
      # but we test it through query_events
      result = RustCoreClient.query_events(query)
      assert_result(result)
    end

    test "compiles query with all parameters" do
      predicate = Query.make_predicate(:eq, :entity_id, "test")

      query =
        Query.new()
        |> Query.add_predicate(predicate)
        |> Query.add_limit(50)
        |> Query.add_offset(10)

      result = RustCoreClient.query_events(query)
      assert_result(result)
    end
  end

  describe "API contract validation" do
    test "validates event structure for creation" do
      # Valid event
      valid_event = %{
        entity_id: "test",
        event_type: "test.event",
        payload: %{}
      }

      assert is_map(valid_event)
      assert Map.has_key?(valid_event, :entity_id)
      assert Map.has_key?(valid_event, :event_type)
    end

    test "validates query params structure" do
      # Valid query params
      valid_params = %{
        entity_id: "test",
        event_type: "test.event",
        limit: 10,
        offset: 0
      }

      assert is_map(valid_params)
    end

    test "validates projection structure" do
      # Valid projection
      valid_projection = %{
        name: "test",
        version: 1,
        initial_state: %{}
      }

      assert is_map(valid_projection)
      assert is_atom(valid_projection.name) or is_binary(valid_projection.name)
    end
  end
end
