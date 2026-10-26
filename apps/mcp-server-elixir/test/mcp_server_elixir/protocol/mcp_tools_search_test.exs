defmodule McpServerElixir.Protocol.McpToolsSearchTest do
  use ExUnit.Case, async: true

  alias McpServerElixir.Protocol.McpTools

  @moduletag :mcp_tools_search

  describe "list_tools/0" do
    test "includes semantic_search_events tool" do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool_names = Enum.map(tools, & &1.name)
      assert "semantic_search_events" in tool_names
    end

    test "includes hybrid_search tool" do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool_names = Enum.map(tools, & &1.name)
      assert "hybrid_search" in tool_names
    end
  end

  describe "semantic_search_events tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "semantic_search_events"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "semantic_search_events"
    end

    test "has description with agent guidance", %{tool: tool} do
      description = tool.description

      # Check for agent guidance sections
      assert description =~ "When to use this tool"
      assert description =~ "Tips for effective queries"
      assert description =~ "natural language"
      assert description =~ "semantic similarity"
    end

    test "has required query parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert schema.required == ["query"]
      assert Map.has_key?(schema.properties, "query")
      assert schema.properties["query"].type == "string"
    end

    test "has optional limit parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "limit")
      assert schema.properties["limit"].type == "number"
    end

    test "has optional threshold parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "threshold")
      assert schema.properties["threshold"].type == "number"
      assert schema.properties["threshold"].description =~ "0.0 to 1.0"
    end

    test "has optional format parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "format")
      assert schema.properties["format"].type == "string"
      assert schema.properties["format"].enum == ["toon", "json"]
    end
  end

  describe "hybrid_search tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "hybrid_search"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "hybrid_search"
    end

    test "has description with agent guidance", %{tool: tool} do
      description = tool.description

      # Check for agent guidance sections
      assert description =~ "When to use this tool"
      assert description =~ "Search strategies"
      assert description =~ "Tips"
      assert description =~ "Reciprocal Rank Fusion"
      assert description =~ "RRF"
    end

    test "has optional semantic_query parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "semantic_query")
      assert schema.properties["semantic_query"].type == "string"
      assert schema.properties["semantic_query"].description =~ "Natural language"
    end

    test "has optional keywords parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "keywords")
      assert schema.properties["keywords"].type == "string"
      assert schema.properties["keywords"].description =~ "BM25"
    end

    test "has filters object with nested properties", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "filters")
      assert schema.properties["filters"].type == "object"

      filters = schema.properties["filters"]
      assert Map.has_key?(filters.properties, "event_type")
      assert Map.has_key?(filters.properties, "entity_id")
      assert Map.has_key?(filters.properties, "time_from")
      assert Map.has_key?(filters.properties, "time_to")
    end

    test "filters have correct types", %{tool: tool} do
      filters = tool.inputSchema.properties["filters"].properties

      assert filters["event_type"].type == "string"
      assert filters["entity_id"].type == "string"
      assert filters["time_from"].type == "string"
      assert filters["time_to"].type == "string"
    end

    test "time filters mention ISO timestamp format", %{tool: tool} do
      filters = tool.inputSchema.properties["filters"].properties

      assert filters["time_from"].description =~ "ISO"
      assert filters["time_to"].description =~ "ISO"
    end

    test "has optional limit parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "limit")
      assert schema.properties["limit"].type == "number"
    end

    test "has optional format parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "format")
      assert schema.properties["format"].type == "string"
      assert schema.properties["format"].enum == ["toon", "json"]
    end

    test "no required parameters (all optional)", %{tool: tool} do
      schema = tool.inputSchema

      # hybrid_search doesn't require any params - you can search with semantic, keywords, filters, or any combination
      refute Map.has_key?(schema, :required)
    end
  end

  describe "tool count" do
    test "list_tools returns 67 tools (43 previous + 6 schema + 8 analytics + 4 developer + 3 fleet-read + 3 view)" do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      assert length(tools) == 67
    end
  end
end
