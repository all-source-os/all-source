defmodule McpServerElixir.Protocol.McpToolsTenantTest do
  use ExUnit.Case, async: true

  alias McpServerElixir.Protocol.McpTools

  @moduletag :mcp_tools_tenant

  # ============================================================================
  # Tool List Tests
  # ============================================================================

  describe "list_tools/0" do
    test "includes all 6 tenant management tools" do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool_names = Enum.map(tools, & &1.name)

      assert "tenant_create" in tool_names
      assert "tenant_update" in tool_names
      assert "tenant_usage" in tool_names
      assert "tenant_quotas" in tool_names
      assert "tenant_suspend" in tool_names
      assert "tenant_export" in tool_names
    end

    test "returns 64 tools total" do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      assert length(tools) == 64
    end
  end

  # ============================================================================
  # tenant_create Tool Definition Tests
  # ============================================================================

  describe "tenant_create tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "tenant_create"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "tenant_create"
    end

    test "has description with usage guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "Common patterns"
      assert tool.description =~ "Performance tips"
    end

    test "description requires admin role", %{tool: tool} do
      assert tool.description =~ "ADMIN ONLY"
    end

    test "description mentions provisioning and onboarding", %{tool: tool} do
      assert tool.description =~ "Provision" or tool.description =~ "provision"
      assert tool.description =~ "Onboarding" or tool.description =~ "onboarding"
    end

    test "has required name parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "name")
      assert schema.properties["name"].type == "string"
      assert "name" in schema.required
    end

    test "has optional tenant_id parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "tenant_id")
      assert schema.properties["tenant_id"].type == "string"
    end

    test "has plan parameter with correct enum", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "plan")
      assert schema.properties["plan"].type == "string"
      assert schema.properties["plan"].enum == ["free", "standard", "professional", "enterprise"]
    end

    test "has max_events_per_day parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "max_events_per_day")
      assert schema.properties["max_events_per_day"].type == "number"
    end

    test "has max_storage_bytes parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "max_storage_bytes")
      assert schema.properties["max_storage_bytes"].type == "number"
    end

    test "has metadata parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "metadata")
      assert schema.properties["metadata"].type == "object"
    end
  end

  # ============================================================================
  # tenant_update Tool Definition Tests
  # ============================================================================

  describe "tenant_update tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "tenant_update"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "tenant_update"
    end

    test "has description with usage guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "Common patterns"
      assert tool.description =~ "Performance tips"
    end

    test "description requires admin role", %{tool: tool} do
      assert tool.description =~ "ADMIN ONLY"
    end

    test "description mentions upgrading and adjusting quotas", %{tool: tool} do
      assert tool.description =~ "Upgrading" or tool.description =~ "upgrading"
      assert tool.description =~ "quota" or tool.description =~ "Quota"
    end

    test "has required tenant_id parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "tenant_id")
      assert schema.properties["tenant_id"].type == "string"
      assert "tenant_id" in schema.required
    end

    test "has optional plan parameter with correct enum", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "plan")
      assert schema.properties["plan"].enum == ["free", "standard", "professional", "enterprise"]
    end

    test "has optional name parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "name")
      assert schema.properties["name"].type == "string"
    end

    test "has metadata parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "metadata")
      assert schema.properties["metadata"].type == "object"
    end
  end

  # ============================================================================
  # tenant_usage Tool Definition Tests
  # ============================================================================

  describe "tenant_usage tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "tenant_usage"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "tenant_usage"
    end

    test "has description with usage guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "Common patterns"
      assert tool.description =~ "Performance tips"
    end

    test "description requires admin role", %{tool: tool} do
      assert tool.description =~ "ADMIN ONLY"
    end

    test "description includes decision guide", %{tool: tool} do
      assert tool.description =~ "Decision guide"
    end

    test "description mentions monitoring and billing", %{tool: tool} do
      assert tool.description =~ "Monitoring" or tool.description =~ "monitoring"
      assert tool.description =~ "billing" or tool.description =~ "Billing"
    end

    test "has required tenant_id parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "tenant_id")
      assert schema.properties["tenant_id"].type == "string"
      assert "tenant_id" in schema.required
    end

    test "has period parameter with correct enum", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "period")
      assert schema.properties["period"].type == "string"
      assert schema.properties["period"].enum == ["1h", "24h", "7d", "30d", "90d"]
    end

    test "has include_breakdown parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "include_breakdown")
      assert schema.properties["include_breakdown"].type == "boolean"
    end
  end

  # ============================================================================
  # tenant_quotas Tool Definition Tests
  # ============================================================================

  describe "tenant_quotas tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "tenant_quotas"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "tenant_quotas"
    end

    test "has description with usage guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "Common patterns"
      assert tool.description =~ "Performance tips"
    end

    test "description requires admin role", %{tool: tool} do
      assert tool.description =~ "ADMIN ONLY"
    end

    test "description mentions key quota types", %{tool: tool} do
      assert tool.description =~ "max_events_per_day"
      assert tool.description =~ "max_storage_bytes"
      assert tool.description =~ "max_api_calls_per_minute"
    end

    test "has required tenant_id parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "tenant_id")
      assert schema.properties["tenant_id"].type == "string"
      assert "tenant_id" in schema.required
    end

    test "has include_usage parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "include_usage")
      assert schema.properties["include_usage"].type == "boolean"
    end
  end

  # ============================================================================
  # tenant_suspend Tool Definition Tests
  # ============================================================================

  describe "tenant_suspend tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "tenant_suspend"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "tenant_suspend"
    end

    test "has description with usage guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "Common patterns"
      assert tool.description =~ "Performance tips"
    end

    test "description requires admin role", %{tool: tool} do
      assert tool.description =~ "ADMIN ONLY"
    end

    test "description includes safety warning", %{tool: tool} do
      assert tool.description =~ "SAFETY WARNING"
      assert tool.description =~ "soft disable" or tool.description =~ "NOT a deletion"
    end

    test "description mentions reactivation", %{tool: tool} do
      assert tool.description =~ "reactivat" or tool.description =~ "Reactivat"
    end

    test "has required tenant_id parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "tenant_id")
      assert schema.properties["tenant_id"].type == "string"
      assert "tenant_id" in schema.required
    end

    test "has required reason parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "reason")
      assert schema.properties["reason"].type == "string"
      assert "reason" in schema.required
    end

    test "has optional notify parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "notify")
      assert schema.properties["notify"].type == "boolean"
    end
  end

  # ============================================================================
  # tenant_export Tool Definition Tests
  # ============================================================================

  describe "tenant_export tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "tenant_export"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "tenant_export"
    end

    test "has description with usage guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "Common patterns"
      assert tool.description =~ "Performance tips"
    end

    test "description requires admin role", %{tool: tool} do
      assert tool.description =~ "ADMIN ONLY"
    end

    test "description includes safety warning", %{tool: tool} do
      assert tool.description =~ "SAFETY WARNING"
    end

    test "description mentions GDPR and data portability", %{tool: tool} do
      assert tool.description =~ "GDPR"
      assert tool.description =~ "portability" or tool.description =~ "migration"
    end

    test "has required tenant_id parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "tenant_id")
      assert schema.properties["tenant_id"].type == "string"
      assert "tenant_id" in schema.required
    end

    test "has format parameter with correct enum", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "format")
      assert schema.properties["format"].type == "string"
      assert schema.properties["format"].enum == ["json", "jsonl", "csv"]
    end

    test "has since and until parameters", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "since")
      assert schema.properties["since"].type == "string"
      assert Map.has_key?(schema.properties, "until")
      assert schema.properties["until"].type == "string"
    end

    test "has include_projections parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "include_projections")
      assert schema.properties["include_projections"].type == "boolean"
    end

    test "has include_metadata parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "include_metadata")
      assert schema.properties["include_metadata"].type == "boolean"
    end
  end
end
