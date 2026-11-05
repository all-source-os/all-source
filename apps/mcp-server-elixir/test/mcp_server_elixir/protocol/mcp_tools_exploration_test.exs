defmodule McpServerElixir.Protocol.McpToolsExplorationTest do
  use ExUnit.Case, async: true

  alias McpServerElixir.Protocol.McpTools

  @moduletag :mcp_tools_exploration

  # ============================================================================
  # Tool Definitions Tests
  # ============================================================================

  describe "list_tools/0" do
    test "includes sample_events tool" do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool_names = Enum.map(tools, & &1.name)
      assert "sample_events" in tool_names
    end

    test "includes quick_stats tool" do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool_names = Enum.map(tools, & &1.name)
      assert "quick_stats" in tool_names
    end
  end

  describe "sample_events tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "sample_events"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "sample_events"
    end

    test "has description with agent guidance", %{tool: tool} do
      description = tool.description

      # Check for agent guidance sections
      assert description =~ "When to use this tool"
      assert description =~ "Common patterns"
      assert description =~ "Performance tips"
      assert description =~ "Decision guide"
    end

    test "description mentions sub-second response optimization", %{tool: tool} do
      assert tool.description =~ "sub-second"
    end

    test "description explains stratification options", %{tool: tool} do
      description = tool.description

      assert description =~ "Stratification options"
      assert description =~ "event_type"
      assert description =~ "entity_id"
      assert description =~ "time"
    end

    test "has sample_size parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "sample_size")
      assert schema.properties["sample_size"].type == "number"
      assert schema.properties["sample_size"].description =~ "1000"
      assert schema.properties["sample_size"].description =~ "10000"
    end

    test "has stratified_by parameter with correct enum", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "stratified_by")
      assert schema.properties["stratified_by"].type == "string"
      assert schema.properties["stratified_by"].enum == ["event_type", "entity_id", "time"]
    end

    test "has optional entity_id filter parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "entity_id")
      assert schema.properties["entity_id"].type == "string"
    end

    test "has optional event_type filter parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "event_type")
      assert schema.properties["event_type"].type == "string"
    end

    test "has optional since parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "since")
      assert schema.properties["since"].type == "string"
    end

    test "has optional until parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "until")
      assert schema.properties["until"].type == "string"
    end

    test "has format parameter with correct enum", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "format")
      assert schema.properties["format"].type == "string"
      assert schema.properties["format"].enum == ["toon", "json"]
    end

    test "has no required parameters", %{tool: tool} do
      schema = tool.inputSchema
      refute Map.has_key?(schema, :required)
    end
  end

  describe "quick_stats tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "quick_stats"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "quick_stats"
    end

    test "has description with agent guidance", %{tool: tool} do
      description = tool.description

      # Check for agent guidance sections
      assert description =~ "When to use this tool"
      assert description =~ "Common patterns"
      assert description =~ "Performance tips"
      assert description =~ "Decision guide"
    end

    test "description emphasizes speed and approximation", %{tool: tool} do
      description = tool.description

      assert description =~ "fast"
      assert description =~ "approximate"
      assert description =~ "Trades precision for speed"
    end

    test "description explains available metrics", %{tool: tool} do
      description = tool.description

      assert description =~ "event_count"
      assert description =~ "unique_entities"
      assert description =~ "event_types"
      assert description =~ "time_range"
    end

    test "description includes accuracy note", %{tool: tool} do
      description = tool.description

      assert description =~ "Accuracy note"
      assert description =~ "approximate"
    end

    test "has metric parameter with correct enum", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "metric")
      assert schema.properties["metric"].type == "string"

      expected_enum = ["event_count", "unique_entities", "event_types", "time_range", "all"]
      assert schema.properties["metric"].enum == expected_enum
    end

    test "has optional entity_id filter parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "entity_id")
      assert schema.properties["entity_id"].type == "string"
    end

    test "has optional event_type filter parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "event_type")
      assert schema.properties["event_type"].type == "string"
    end

    test "has no required parameters", %{tool: tool} do
      schema = tool.inputSchema
      refute Map.has_key?(schema, :required)
    end
  end

  # ============================================================================
  # Tool Descriptions Quality Tests
  # ============================================================================

  describe "sample_events description quality" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "sample_events"))
      {:ok, tool: tool}
    end

    test "describes use cases for data exploration", %{tool: tool} do
      assert tool.description =~ "exploration"
      assert tool.description =~ "unfamiliar dataset"
    end

    test "explains stratification strategies", %{tool: tool} do
      description = tool.description

      # event_type stratification
      assert description =~ "event_type" and description =~ "evenly"

      # entity_id stratification
      assert description =~ "entity_id"

      # time stratification
      assert description =~ "time" or description =~ "temporal"
    end

    test "provides performance guidance", %{tool: tool} do
      description = tool.description

      assert description =~ "FAST"
      assert description =~ "before expensive full scans"
    end

    test "includes decision guide comparing to other tools", %{tool: tool} do
      description = tool.description

      assert description =~ "query_events"
      assert description =~ "quick_stats"
    end
  end

  describe "quick_stats description quality" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "quick_stats"))
      {:ok, tool: tool}
    end

    test "describes speed advantage", %{tool: tool} do
      assert tool.description =~ "FASTEST"
      assert tool.description =~ "pre-computed"
    end

    test "explains approximation tradeoff", %{tool: tool} do
      description = tool.description

      assert description =~ "approximate"
      assert description =~ "5%"
    end

    test "compares to get_stats for exact counts", %{tool: tool} do
      description = tool.description

      assert description =~ "get_stats"
      assert description =~ "exact"
    end

    test "provides guidance for each metric type", %{tool: tool} do
      description = tool.description

      assert description =~ "event_count"
      assert description =~ "unique_entities"
      assert description =~ "event_types"
      assert description =~ "time_range"
    end
  end

  # ============================================================================
  # Tool Count Tests
  # ============================================================================

  describe "tool count" do
    test "list_tools returns 64 tools" do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      assert length(tools) == 64
    end
  end

  # ============================================================================
  # Decision Tree Integration Tests
  # ============================================================================

  describe "exploration tools in decision workflow" do
    test "sample_events mentions quick_stats as alternative for stats-only" do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "sample_events"))

      assert tool.description =~ "quick_stats"
    end

    test "quick_stats mentions sample_events for getting actual events" do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "quick_stats"))

      assert tool.description =~ "sample_events"
    end

    test "both tools reference get_stats for comparison" do
      tools = McpTools.list_tools(%{control_plane_enabled: true})

      sample_tool = Enum.find(tools, &(&1.name == "sample_events"))
      stats_tool = Enum.find(tools, &(&1.name == "quick_stats"))

      # sample_events should mention query_events as full alternative
      assert sample_tool.description =~ "query_events"

      # quick_stats should mention get_stats for exact counts
      assert stats_tool.description =~ "get_stats"
    end
  end
end
