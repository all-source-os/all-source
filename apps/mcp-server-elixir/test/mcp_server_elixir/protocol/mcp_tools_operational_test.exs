defmodule McpServerElixir.Protocol.McpToolsOperationalTest do
  use ExUnit.Case, async: true

  alias McpServerElixir.Protocol.McpTools

  @moduletag :mcp_tools_operational

  # ============================================================================
  # Tool List Tests
  # ============================================================================

  describe "list_tools/0" do
    test "includes all 10 operational tools" do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool_names = Enum.map(tools, & &1.name)

      assert "compact_storage" in tool_names
      assert "storage_stats" in tool_names
      assert "partition_info" in tool_names
      assert "wal_status" in tool_names
      assert "backup_create" in tool_names
      assert "backup_restore" in tool_names
      assert "backup_list" in tool_names
      assert "health_deep" in tool_names
      assert "performance_report" in tool_names
      assert "audit_log" in tool_names
    end

    test "returns 67 tools total" do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      assert length(tools) == 67
    end
  end

  # ============================================================================
  # compact_storage Tool Definition Tests
  # ============================================================================

  describe "compact_storage tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "compact_storage"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "compact_storage"
    end

    test "has description with usage guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "Common patterns"
      assert tool.description =~ "Performance tips"
    end

    test "description includes safety warning", %{tool: tool} do
      assert tool.description =~ "SAFETY WARNING"
      assert tool.description =~ "I/O intensive"
    end

    test "description mentions disk space and compaction", %{tool: tool} do
      assert tool.description =~ "disk space"
      assert tool.description =~ "compaction" or tool.description =~ "Compaction"
    end

    test "has optional tenant_id parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "tenant_id")
      assert schema.properties["tenant_id"].type == "string"
    end

    test "has optional partition_id parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "partition_id")
      assert schema.properties["partition_id"].type == "string"
    end

    test "has dry_run parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "dry_run")
      assert schema.properties["dry_run"].type == "boolean"
    end

    test "has no required parameters", %{tool: tool} do
      schema = tool.inputSchema
      refute Map.has_key?(schema, :required)
    end
  end

  # ============================================================================
  # storage_stats Tool Definition Tests
  # ============================================================================

  describe "storage_stats tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "storage_stats"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "storage_stats"
    end

    test "has description with usage guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "Common patterns"
      assert tool.description =~ "Performance tips"
    end

    test "description mentions capacity planning and cost allocation", %{tool: tool} do
      assert tool.description =~ "Capacity planning" or tool.description =~ "capacity planning"
      assert tool.description =~ "cost"
    end

    test "has group_by parameter with correct enum", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "group_by")
      assert schema.properties["group_by"].type == "string"
      assert schema.properties["group_by"].enum == ["tenant", "event_type", "partition"]
    end

    test "has optional tenant_id parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "tenant_id")
      assert schema.properties["tenant_id"].type == "string"
    end

    test "has refresh parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "refresh")
      assert schema.properties["refresh"].type == "boolean"
    end
  end

  # ============================================================================
  # partition_info Tool Definition Tests
  # ============================================================================

  describe "partition_info tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "partition_info"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "partition_info"
    end

    test "has description with usage guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "Common patterns"
      assert tool.description =~ "Performance tips"
    end

    test "description mentions partition health and distribution", %{tool: tool} do
      assert tool.description =~ "health"
      assert tool.description =~ "distribution"
    end

    test "has optional partition_id parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "partition_id")
      assert schema.properties["partition_id"].type == "string"
    end

    test "has include_replicas parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "include_replicas")
      assert schema.properties["include_replicas"].type == "boolean"
    end
  end

  # ============================================================================
  # wal_status Tool Definition Tests
  # ============================================================================

  describe "wal_status tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "wal_status"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "wal_status"
    end

    test "has description with usage guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "Common patterns"
      assert tool.description =~ "Performance tips"
    end

    test "description mentions WAL and replication lag", %{tool: tool} do
      assert tool.description =~ "Write-Ahead Log" or tool.description =~ "WAL"
      assert tool.description =~ "replication lag" or tool.description =~ "lag"
    end

    test "has empty properties (no parameters needed)", %{tool: tool} do
      schema = tool.inputSchema
      assert schema.properties == %{}
    end
  end

  # ============================================================================
  # backup_create Tool Definition Tests
  # ============================================================================

  describe "backup_create tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "backup_create"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "backup_create"
    end

    test "has description with usage guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "Common patterns"
      assert tool.description =~ "Performance tips"
    end

    test "description includes safety warning", %{tool: tool} do
      assert tool.description =~ "SAFETY WARNING"
      assert tool.description =~ "I/O intensive"
    end

    test "description mentions disaster recovery", %{tool: tool} do
      assert tool.description =~ "disaster recovery"
    end

    test "has label parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "label")
      assert schema.properties["label"].type == "string"
    end

    test "has optional tenant_id parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "tenant_id")
      assert schema.properties["tenant_id"].type == "string"
    end

    test "has incremental parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "incremental")
      assert schema.properties["incremental"].type == "boolean"
    end
  end

  # ============================================================================
  # backup_restore Tool Definition Tests
  # ============================================================================

  describe "backup_restore tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "backup_restore"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "backup_restore"
    end

    test "has description with usage guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "Common patterns"
      assert tool.description =~ "Performance tips"
    end

    test "description includes destructive operation warning", %{tool: tool} do
      assert tool.description =~ "DESTRUCTIVE"
      assert tool.description =~ "REPLACES current data"
    end

    test "description mentions data loss risk", %{tool: tool} do
      assert tool.description =~ "LOST"
    end

    test "has required backup_id parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert "backup_id" in schema.required
      assert Map.has_key?(schema.properties, "backup_id")
      assert schema.properties["backup_id"].type == "string"
    end

    test "has required confirm parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert "confirm" in schema.required
      assert Map.has_key?(schema.properties, "confirm")
      assert schema.properties["confirm"].type == "boolean"
    end

    test "has dry_run parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "dry_run")
      assert schema.properties["dry_run"].type == "boolean"
    end

    test "has optional tenant_id parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "tenant_id")
      assert schema.properties["tenant_id"].type == "string"
    end
  end

  # ============================================================================
  # backup_list Tool Definition Tests
  # ============================================================================

  describe "backup_list tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "backup_list"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "backup_list"
    end

    test "has description with usage guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "Common patterns"
      assert tool.description =~ "Performance tips"
    end

    test "description mentions finding backups for restore", %{tool: tool} do
      assert tool.description =~ "restore"
    end

    test "has optional tenant_id parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "tenant_id")
      assert schema.properties["tenant_id"].type == "string"
    end

    test "has limit parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "limit")
      assert schema.properties["limit"].type == "number"
    end
  end

  # ============================================================================
  # health_deep Tool Definition Tests
  # ============================================================================

  describe "health_deep tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "health_deep"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "health_deep"
    end

    test "has description with usage guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "Common patterns"
    end

    test "description mentions comprehensive health check", %{tool: tool} do
      assert tool.description =~ "comprehensive" or tool.description =~ "Comprehensive"
      assert tool.description =~ "health"
    end

    test "description includes decision guide", %{tool: tool} do
      assert tool.description =~ "Decision guide"
      assert tool.description =~ "get_stats"
      assert tool.description =~ "get_cluster_status"
    end

    test "has empty properties (no parameters needed)", %{tool: tool} do
      schema = tool.inputSchema
      assert schema.properties == %{}
    end
  end

  # ============================================================================
  # performance_report Tool Definition Tests
  # ============================================================================

  describe "performance_report tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "performance_report"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "performance_report"
    end

    test "has description with usage guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "Common patterns"
      assert tool.description =~ "Performance tips"
    end

    test "description mentions key metrics", %{tool: tool} do
      assert tool.description =~ "latency"
      assert tool.description =~ "throughput"
      assert tool.description =~ "cache"
    end

    test "has period parameter with correct enum", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "period")
      assert schema.properties["period"].type == "string"
      assert schema.properties["period"].enum == ["1h", "6h", "24h", "7d", "30d"]
    end

    test "has optional tenant_id parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "tenant_id")
      assert schema.properties["tenant_id"].type == "string"
    end
  end

  # ============================================================================
  # audit_log Tool Definition Tests
  # ============================================================================

  describe "audit_log tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "audit_log"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "audit_log"
    end

    test "has description with usage guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "Common patterns"
      assert tool.description =~ "Performance tips"
    end

    test "description mentions compliance frameworks", %{tool: tool} do
      assert tool.description =~ "GDPR"
      assert tool.description =~ "SOC2"
    end

    test "description mentions forensic analysis", %{tool: tool} do
      assert tool.description =~ "forensic" or tool.description =~ "Forensic"
    end

    test "has action filter parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "action")
      assert schema.properties["action"].type == "string"
    end

    test "has actor filter parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "actor")
      assert schema.properties["actor"].type == "string"
    end

    test "has entity_id filter parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "entity_id")
      assert schema.properties["entity_id"].type == "string"
    end

    test "has time range parameters", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "since")
      assert Map.has_key?(schema.properties, "until")
    end

    test "has limit parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "limit")
      assert schema.properties["limit"].type == "number"
    end
  end

  # ============================================================================
  # Description Quality Tests - All Operational Tools
  # ============================================================================

  describe "operational tool description quality" do
    @operational_tools [
      "compact_storage",
      "storage_stats",
      "partition_info",
      "wal_status",
      "backup_create",
      "backup_restore",
      "backup_list",
      "health_deep",
      "performance_report",
      "audit_log"
    ]

    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      {:ok, tools: tools}
    end

    test "all tools have descriptions with 'When to use this tool' section", %{tools: tools} do
      for tool_name <- @operational_tools do
        tool = Enum.find(tools, &(&1.name == tool_name))

        assert tool.description =~ "When to use this tool",
               "#{tool_name} missing 'When to use this tool' section"
      end
    end

    test "all tools have descriptions with 'Common patterns' section", %{tools: tools} do
      for tool_name <- @operational_tools do
        tool = Enum.find(tools, &(&1.name == tool_name))

        assert tool.description =~ "Common patterns",
               "#{tool_name} missing 'Common patterns' section"
      end
    end

    test "destructive tools include safety warnings", %{tools: tools} do
      destructive_tools = ["compact_storage", "backup_create", "backup_restore"]

      for tool_name <- destructive_tools do
        tool = Enum.find(tools, &(&1.name == tool_name))

        assert tool.description =~ "SAFETY WARNING" or tool.description =~ "DESTRUCTIVE",
               "#{tool_name} should include safety warning"
      end
    end

    test "backup_restore requires confirmation", %{tools: tools} do
      tool = Enum.find(tools, &(&1.name == "backup_restore"))
      schema = tool.inputSchema
      assert "confirm" in schema.required
    end
  end
end
