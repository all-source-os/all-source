defmodule McpServerElixir.Protocol.McpToolsDeveloperTest do
  use ExUnit.Case, async: true

  alias McpServerElixir.Protocol.McpTools

  @moduletag :mcp_tools_developer

  # ============================================================================
  # Tool List Tests
  # ============================================================================

  describe "list_tools/0" do
    test "includes all 4 developer experience tools" do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool_names = Enum.map(tools, & &1.name)

      assert "generate_client" in tool_names
      assert "mock_events" in tool_names
      assert "debug_query" in tool_names
      assert "benchmark_query" in tool_names
    end

    test "returns 64 tools total" do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      assert length(tools) == 64
    end
  end

  # ============================================================================
  # generate_client Tool Definition Tests
  # ============================================================================

  describe "generate_client tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "generate_client"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "generate_client"
    end

    test "has description with agent guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "Common patterns"
      assert tool.description =~ "Performance tips"
    end

    test "description mentions SDK and code generation", %{tool: tool} do
      assert tool.description =~ "SDK" or tool.description =~ "client"
      assert tool.description =~ "generat" or tool.description =~ "Generat"
    end

    test "has required language parameter with correct enum", %{tool: tool} do
      schema = tool.inputSchema
      assert "language" in schema.required
      assert Map.has_key?(schema.properties, "language")

      assert schema.properties["language"].enum == [
               "typescript",
               "python",
               "go",
               "rust",
               "elixir"
             ]
    end

    test "has operations parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "operations")
      assert schema.properties["operations"].type == "array"
    end

    test "has style parameter with correct enum", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "style")
      assert schema.properties["style"].enum == ["minimal", "full", "example"]
    end

    test "has include_schemas parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "include_schemas")
      assert schema.properties["include_schemas"].type == "boolean"
    end
  end

  # ============================================================================
  # mock_events Tool Definition Tests
  # ============================================================================

  describe "mock_events tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "mock_events"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "mock_events"
    end

    test "has description with agent guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "Common patterns"
      assert tool.description =~ "Performance tips"
    end

    test "description includes safety warning", %{tool: tool} do
      assert tool.description =~ "SAFETY WARNING"
      assert tool.description =~ "REAL events"
    end

    test "description mentions test data generation", %{tool: tool} do
      assert tool.description =~ "test" or tool.description =~ "mock"
    end

    test "has required event_type parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert "event_type" in schema.required
      assert schema.properties["event_type"].type == "string"
    end

    test "has count parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "count")
      assert schema.properties["count"].type == "number"
    end

    test "has entity_count parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "entity_count")
      assert schema.properties["entity_count"].type == "number"
    end

    test "has template parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "template")
      assert schema.properties["template"].type == "object"
    end

    test "has dry_run parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "dry_run")
      assert schema.properties["dry_run"].type == "boolean"
    end

    test "has time_range parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "time_range")
      assert schema.properties["time_range"].type == "object"
    end

    test "has tenant_id parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "tenant_id")
      assert schema.properties["tenant_id"].type == "string"
    end
  end

  # ============================================================================
  # debug_query Tool Definition Tests
  # ============================================================================

  describe "debug_query tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "debug_query"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "debug_query"
    end

    test "has description with agent guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "Common patterns"
      assert tool.description =~ "Performance tips"
    end

    test "description mentions execution plan", %{tool: tool} do
      assert tool.description =~ "execution plan" or tool.description =~ "Execution plan"
    end

    test "description mentions optimization", %{tool: tool} do
      assert tool.description =~ "optimi" or tool.description =~ "Optimi"
    end

    test "has entity_id parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "entity_id")
      assert schema.properties["entity_id"].type == "string"
    end

    test "has event_type parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "event_type")
      assert schema.properties["event_type"].type == "string"
    end

    test "has time range parameters", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "since")
      assert Map.has_key?(schema.properties, "until")
    end

    test "has no required parameters", %{tool: tool} do
      schema = tool.inputSchema
      refute Map.has_key?(schema, :required)
    end
  end

  # ============================================================================
  # benchmark_query Tool Definition Tests
  # ============================================================================

  describe "benchmark_query tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "benchmark_query"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "benchmark_query"
    end

    test "has description with agent guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "Common patterns"
      assert tool.description =~ "Performance tips"
    end

    test "description includes safety warning", %{tool: tool} do
      assert tool.description =~ "SAFETY WARNING"
      assert tool.description =~ "REAL queries"
    end

    test "description mentions latency percentiles", %{tool: tool} do
      assert tool.description =~ "p50"
      assert tool.description =~ "p95"
      assert tool.description =~ "p99"
    end

    test "has required query parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert "query" in schema.required
      assert Map.has_key?(schema.properties, "query")
      assert schema.properties["query"].type == "object"
    end

    test "has iterations parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "iterations")
      assert schema.properties["iterations"].type == "number"
    end

    test "has warmup parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "warmup")
      assert schema.properties["warmup"].type == "number"
    end
  end

  # ============================================================================
  # Description Quality Tests - All Developer Tools
  # ============================================================================

  describe "developer tool description quality" do
    @developer_tools [
      "generate_client",
      "mock_events",
      "debug_query",
      "benchmark_query"
    ]

    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      {:ok, tools: tools}
    end

    test "all tools have descriptions with 'When to use this tool' section", %{tools: tools} do
      for tool_name <- @developer_tools do
        tool = Enum.find(tools, &(&1.name == tool_name))

        assert tool.description =~ "When to use this tool",
               "#{tool_name} missing 'When to use this tool' section"
      end
    end

    test "all tools have descriptions with 'Common patterns' section", %{tools: tools} do
      for tool_name <- @developer_tools do
        tool = Enum.find(tools, &(&1.name == tool_name))

        assert tool.description =~ "Common patterns",
               "#{tool_name} missing 'Common patterns' section"
      end
    end

    test "all tools have descriptions with 'Performance tips' section", %{tools: tools} do
      for tool_name <- @developer_tools do
        tool = Enum.find(tools, &(&1.name == tool_name))

        assert tool.description =~ "Performance tips",
               "#{tool_name} missing 'Performance tips' section"
      end
    end

    test "destructive tools include safety warnings", %{tools: tools} do
      destructive_tools = ["mock_events", "benchmark_query"]

      for tool_name <- destructive_tools do
        tool = Enum.find(tools, &(&1.name == tool_name))

        assert tool.description =~ "SAFETY WARNING",
               "#{tool_name} should include safety warning"
      end
    end
  end
end
