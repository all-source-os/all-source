defmodule McpServerElixir.Protocol.McpToolsSchemaTest do
  use ExUnit.Case, async: true

  alias McpServerElixir.Protocol.McpTools

  @moduletag :mcp_tools_schema

  # ============================================================================
  # Tool List Tests
  # ============================================================================

  describe "list_tools/0" do
    test "includes all 6 schema tools" do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool_names = Enum.map(tools, & &1.name)

      assert "register_schema" in tool_names
      assert "validate_schema" in tool_names
      assert "migrate_schema" in tool_names
      assert "list_schemas" in tool_names
      assert "infer_schema" in tool_names
      assert "schema_diff" in tool_names
    end

    test "returns 67 tools total (43 previous + 6 schema + 8 analytics + 4 developer + 3 fleet-read + 3 view)" do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      assert length(tools) == 67
    end
  end

  # ============================================================================
  # register_schema Tool Definition Tests
  # ============================================================================

  describe "register_schema tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "register_schema"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "register_schema"
    end

    test "has description with agent guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "Common patterns"
      assert tool.description =~ "Performance tips"
      assert tool.description =~ "Decision guide"
    end

    test "description mentions data contracts and governance", %{tool: tool} do
      assert tool.description =~ "data contracts"
      assert tool.description =~ "governance"
    end

    test "has required subject parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert "subject" in schema.required
      assert Map.has_key?(schema.properties, "subject")
      assert schema.properties["subject"].type == "string"
    end

    test "has required definition parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert "definition" in schema.required
      assert Map.has_key?(schema.properties, "definition")
      assert schema.properties["definition"].type == "object"
    end

    test "has compatibility parameter with correct enum", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "compatibility")
      assert schema.properties["compatibility"].type == "string"
      assert schema.properties["compatibility"].enum == ["none", "backward", "forward", "full"]
    end

    test "has optional tags parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "tags")
      assert schema.properties["tags"].type == "array"
    end

    test "has optional description parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "description")
      assert schema.properties["description"].type == "string"
    end
  end

  # ============================================================================
  # validate_schema Tool Definition Tests
  # ============================================================================

  describe "validate_schema tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "validate_schema"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "validate_schema"
    end

    test "has description with agent guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "Common patterns"
      assert tool.description =~ "Performance tips"
    end

    test "description mentions validation and debugging", %{tool: tool} do
      assert tool.description =~ "validat" or tool.description =~ "Validat"
      assert tool.description =~ "debug" or tool.description =~ "Debug"
    end

    test "has required subject parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert "subject" in schema.required
      assert schema.properties["subject"].type == "string"
    end

    test "has required payload parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert "payload" in schema.required
      assert schema.properties["payload"].type == "object"
    end

    test "has optional version parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "version")
      assert schema.properties["version"].type == "number"
    end
  end

  # ============================================================================
  # migrate_schema Tool Definition Tests
  # ============================================================================

  describe "migrate_schema tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "migrate_schema"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "migrate_schema"
    end

    test "has description with agent guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "Common patterns"
      assert tool.description =~ "How it works"
    end

    test "description mentions client-side computation", %{tool: tool} do
      assert tool.description =~ "client-side"
    end

    test "has required subject parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert "subject" in schema.required
      assert schema.properties["subject"].type == "string"
    end

    test "has required new_definition parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert "new_definition" in schema.required
      assert schema.properties["new_definition"].type == "object"
    end

    test "has transformations parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "transformations")
      assert schema.properties["transformations"].type == "array"
    end

    test "has dry_run parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "dry_run")
      assert schema.properties["dry_run"].type == "boolean"
    end
  end

  # ============================================================================
  # list_schemas Tool Definition Tests
  # ============================================================================

  describe "list_schemas tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "list_schemas"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "list_schemas"
    end

    test "has description with agent guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "Common patterns"
      assert tool.description =~ "Performance tips"
    end

    test "description mentions discovery and governance", %{tool: tool} do
      assert tool.description =~ "discover" or tool.description =~ "Discover"
      assert tool.description =~ "governance"
    end

    test "has optional tag parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "tag")
      assert schema.properties["tag"].type == "string"
    end

    test "has include_versions parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "include_versions")
      assert schema.properties["include_versions"].type == "boolean"
    end

    test "has no required parameters", %{tool: tool} do
      schema = tool.inputSchema
      refute Map.has_key?(schema, :required)
    end
  end

  # ============================================================================
  # infer_schema Tool Definition Tests
  # ============================================================================

  describe "infer_schema tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "infer_schema"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "infer_schema"
    end

    test "has description with agent guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "Common patterns"
      assert tool.description =~ "Limitations"
    end

    test "description mentions client-side inference", %{tool: tool} do
      assert tool.description =~ "client-side"
      assert tool.description =~ "Auto-generate"
    end

    test "has required event_type parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert "event_type" in schema.required
      assert schema.properties["event_type"].type == "string"
    end

    test "has optional sample_size parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "sample_size")
      assert schema.properties["sample_size"].type == "number"
    end

    test "has optional entity_id parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "entity_id")
      assert schema.properties["entity_id"].type == "string"
    end
  end

  # ============================================================================
  # schema_diff Tool Definition Tests
  # ============================================================================

  describe "schema_diff tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "schema_diff"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "schema_diff"
    end

    test "has description with agent guidance", %{tool: tool} do
      assert tool.description =~ "How it works"
      assert tool.description =~ "Common patterns"
      assert tool.description =~ "Important"
    end

    test "description mentions client-side diff", %{tool: tool} do
      assert tool.description =~ "client-side"
      assert tool.description =~ "Compare"
    end

    test "has required subject parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert "subject" in schema.required
      assert schema.properties["subject"].type == "string"
    end

    test "has optional version_a and version_b parameters", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "version_a")
      assert Map.has_key?(schema.properties, "version_b")
      assert schema.properties["version_a"].type == "number"
      assert schema.properties["version_b"].type == "number"
    end

    test "has optional proposed parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "proposed")
      assert schema.properties["proposed"].type == "object"
    end
  end

  # ============================================================================
  # Description Quality Tests - All Schema Tools
  # ============================================================================

  describe "schema tool description quality" do
    @schema_tools [
      "register_schema",
      "validate_schema",
      "migrate_schema",
      "list_schemas",
      "infer_schema",
      "schema_diff"
    ]

    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      {:ok, tools: tools}
    end

    test "all tools have descriptions with usage guidance section", %{tools: tools} do
      for tool_name <- @schema_tools do
        tool = Enum.find(tools, &(&1.name == tool_name))

        assert tool.description =~ "When to use this tool" or
                 tool.description =~ "How it works",
               "#{tool_name} missing usage guidance section"
      end
    end

    test "all tools have descriptions with 'Common patterns' section", %{tools: tools} do
      for tool_name <- @schema_tools do
        tool = Enum.find(tools, &(&1.name == tool_name))

        assert tool.description =~ "Common patterns",
               "#{tool_name} missing 'Common patterns' section"
      end
    end

    test "all tools have descriptions with decision guide", %{tools: tools} do
      for tool_name <- @schema_tools do
        tool = Enum.find(tools, &(&1.name == tool_name))

        assert tool.description =~ "Decision guide",
               "#{tool_name} missing 'Decision guide' section"
      end
    end
  end
end
