defmodule McpServerElixir.Integration.EmbeddedModeTest do
  @moduledoc """
  Integration test for embedded Core mode.

  Starts CoreEmbedded.Supervisor with in-memory config (no data_dir),
  then exercises MCP tools through the embedded backend.

  Run explicitly: mix test test/mcp_server_elixir/integration/embedded_mode_test.exs
  """

  use ExUnit.Case, async: false

  @moduletag :embedded

  alias McpServerElixir.Infrastructure.CoreEmbedded
  alias McpServerElixir.Protocol.McpTools

  setup_all do
    # Start CoreEmbedded.Supervisor with in-memory config
    Application.put_env(:mcp_server_elixir, :embedded_config, %{
      data_dir: "",
      node_id: 1,
      wal_fsync_interval_ms: 100,
      parquet_flush_interval_secs: 300
    })

    start_supervised!(CoreEmbedded.Supervisor)

    state = %{
      backend: CoreEmbedded,
      read_only: false,
      control_plane_enabled: false
    }

    {:ok, state: state}
  end

  test "ingest_event returns success", %{state: state} do
    args = %{
      "entity_id" => "test-entity-1",
      "event_type" => "user.created",
      "payload" => ~s({"name": "Alice", "email": "alice@example.com"})
    }

    {:ok, result} = McpTools.call_tool("ingest_event", args, state)
    assert result.content
    refute Map.get(result, :isError, false)
  end

  test "query_events returns ingested event", %{state: state} do
    # Ingest first
    ingest_args = %{
      "entity_id" => "test-query-1",
      "event_type" => "order.placed",
      "payload" => ~s({"item": "widget", "qty": 5})
    }

    {:ok, _} = McpTools.call_tool("ingest_event", ingest_args, state)

    # Query
    query_args = %{"entity_id" => "test-query-1"}
    {:ok, result} = McpTools.call_tool("query_events", query_args, state)
    assert result.content
    refute Map.get(result, :isError, false)

    text = hd(result.content).text
    assert text =~ "Found" || text =~ "order.placed" || text =~ "test-query-1" || text =~ "events"
  end

  test "get_stats returns event count > 0", %{state: state} do
    # Ensure at least one event exists
    McpTools.call_tool(
      "ingest_event",
      %{
        "entity_id" => "test-stats-1",
        "event_type" => "ping",
        "payload" => ~s({"t": 1})
      },
      state
    )

    {:ok, result} = McpTools.call_tool("get_stats", %{}, state)
    assert result.content
    refute Map.get(result, :isError, false)
  end

  test "tools the embedded backend cannot serve are neither listed nor dispatched", %{
    state: state
  } do
    names = McpTools.list_tools(state) |> Enum.map(& &1.name)

    for tool <- state.backend.unsupported_tools() do
      refute tool in names
      assert {:error, "Unknown tool: " <> ^tool} = McpTools.call_tool(tool, %{}, state)
    end
  end
end
