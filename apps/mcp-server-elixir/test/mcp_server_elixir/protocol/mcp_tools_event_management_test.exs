defmodule McpServerElixir.Protocol.McpToolsEventManagementTest do
  use ExUnit.Case, async: true

  alias McpServerElixir.Protocol.McpTools

  @moduletag :mcp_tools_event_management

  # ============================================================================
  # Tool List Tests
  # ============================================================================

  describe "list_tools/0" do
    test "includes all 8 event management tools" do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool_names = Enum.map(tools, & &1.name)

      assert "delete_events" in tool_names
      assert "archive_events" in tool_names
      assert "restore_events" in tool_names
      assert "export_events" in tool_names
      assert "import_events" in tool_names
      assert "clone_entity" in tool_names
      assert "merge_entities" in tool_names
      assert "split_entity" in tool_names
    end

    test "returns 64 tools total" do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      assert length(tools) == 64
    end
  end

  # ============================================================================
  # delete_events Tool Definition Tests
  # ============================================================================

  describe "delete_events tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "delete_events"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "delete_events"
    end

    test "has description with usage guidance", %{tool: tool} do
      description = tool.description

      assert description =~ "When to use this tool"
      assert description =~ "Common patterns"
      assert description =~ "Performance tips"
    end

    test "description mentions GDPR/CCPA compliance", %{tool: tool} do
      assert tool.description =~ "GDPR"
      assert tool.description =~ "CCPA"
    end

    test "description explains soft delete behavior", %{tool: tool} do
      assert tool.description =~ "soft delete" or tool.description =~ "SOFT DELETE"
      assert tool.description =~ "tombstone"
      assert tool.description =~ "restore_events"
    end

    test "has required reason parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert "reason" in schema.required
      assert Map.has_key?(schema.properties, "reason")
      assert schema.properties["reason"].type == "string"
    end

    test "has optional entity_id parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "entity_id")
      assert schema.properties["entity_id"].type == "string"
    end

    test "has optional event_ids array parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "event_ids")
      assert schema.properties["event_ids"].type == "array"
    end

    test "has optional time range parameters", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "since")
      assert Map.has_key?(schema.properties, "until")
    end

    test "has dry_run parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "dry_run")
      assert schema.properties["dry_run"].type == "boolean"
    end
  end

  # ============================================================================
  # archive_events Tool Definition Tests
  # ============================================================================

  describe "archive_events tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "archive_events"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "archive_events"
    end

    test "has description with usage guidance", %{tool: tool} do
      description = tool.description

      assert description =~ "When to use this tool"
      assert description =~ "Common patterns"
      assert description =~ "Performance tips"
    end

    test "description mentions cold storage and cost optimization", %{tool: tool} do
      assert tool.description =~ "cold storage"
      assert tool.description =~ "cost"
    end

    test "has required entity_id and reason parameters", %{tool: tool} do
      schema = tool.inputSchema
      assert "entity_id" in schema.required
      assert "reason" in schema.required
    end

    test "has optional older_than parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "older_than")
      assert schema.properties["older_than"].type == "string"
    end

    test "has retention_days parameter with default 365", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "retention_days")
      assert schema.properties["retention_days"].type == "number"
      assert schema.properties["retention_days"].description =~ "365"
    end

    test "has dry_run parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "dry_run")
      assert schema.properties["dry_run"].type == "boolean"
    end
  end

  # ============================================================================
  # restore_events Tool Definition Tests
  # ============================================================================

  describe "restore_events tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "restore_events"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "restore_events"
    end

    test "has description with usage guidance", %{tool: tool} do
      description = tool.description

      assert description =~ "When to use this tool"
      assert description =~ "Common patterns"
    end

    test "description mentions recovering deleted and archived events", %{tool: tool} do
      assert tool.description =~ "Recover" or tool.description =~ "Restor"
      assert tool.description =~ "deleted"
      assert tool.description =~ "archived"
    end

    test "has required reason parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert "reason" in schema.required
    end

    test "has status parameter with correct enum values", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "status")
      assert schema.properties["status"].type == "string"
      assert schema.properties["status"].enum == ["deleted", "archived", "all"]
    end

    test "has optional entity_id and event_ids parameters", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "entity_id")
      assert Map.has_key?(schema.properties, "event_ids")
    end

    test "has dry_run parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "dry_run")
    end
  end

  # ============================================================================
  # export_events Tool Definition Tests
  # ============================================================================

  describe "export_events tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "export_events"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "export_events"
    end

    test "has description with usage guidance", %{tool: tool} do
      description = tool.description

      assert description =~ "When to use this tool"
      assert description =~ "Common patterns"
      assert description =~ "Performance tips"
    end

    test "description mentions supported formats", %{tool: tool} do
      description = tool.description

      assert description =~ "json"
      assert description =~ "jsonl"
      assert description =~ "csv"
      assert description =~ "parquet"
    end

    test "has format parameter with correct enum", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "format")
      assert schema.properties["format"].type == "string"
      assert schema.properties["format"].enum == ["json", "jsonl", "csv", "parquet"]
    end

    test "has compress parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "compress")
      assert schema.properties["compress"].type == "boolean"
    end

    test "has include_metadata parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "include_metadata")
      assert schema.properties["include_metadata"].type == "boolean"
    end

    test "has limit parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "limit")
      assert schema.properties["limit"].type == "number"
    end

    test "has no required parameters", %{tool: tool} do
      schema = tool.inputSchema
      refute Map.has_key?(schema, :required)
    end
  end

  # ============================================================================
  # import_events Tool Definition Tests
  # ============================================================================

  describe "import_events tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "import_events"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "import_events"
    end

    test "has description with usage guidance", %{tool: tool} do
      description = tool.description

      assert description =~ "When to use this tool"
      assert description =~ "Common patterns"
    end

    test "description mentions migration and bulk import", %{tool: tool} do
      assert tool.description =~ "Migrat" or tool.description =~ "migrat"
      assert tool.description =~ "Bulk" or tool.description =~ "bulk"
    end

    test "description shows event structure example", %{tool: tool} do
      description = tool.description

      assert description =~ "event_type"
      assert description =~ "entity_id"
      assert description =~ "payload"
    end

    test "has required data parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert "data" in schema.required
      assert Map.has_key?(schema.properties, "data")
      assert schema.properties["data"].type == "array"
    end

    test "has format parameter with correct enum", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "format")
      assert schema.properties["format"].enum == ["json", "jsonl", "csv"]
    end

    test "has entity_id_prefix parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "entity_id_prefix")
      assert schema.properties["entity_id_prefix"].type == "string"
    end

    test "has validate_only parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "validate_only")
      assert schema.properties["validate_only"].type == "boolean"
    end

    test "has skip_duplicates parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "skip_duplicates")
      assert schema.properties["skip_duplicates"].type == "boolean"
    end

    test "has batch_size parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "batch_size")
      assert schema.properties["batch_size"].type == "number"
    end
  end

  # ============================================================================
  # clone_entity Tool Definition Tests
  # ============================================================================

  describe "clone_entity tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "clone_entity"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "clone_entity"
    end

    test "has description with usage guidance", %{tool: tool} do
      description = tool.description

      assert description =~ "When to use this tool"
      assert description =~ "Common patterns"
    end

    test "description mentions test data and templates", %{tool: tool} do
      assert tool.description =~ "test"
      assert tool.description =~ "template"
    end

    test "description explains cloning process", %{tool: tool} do
      description = tool.description

      assert description =~ "copied"
      assert description =~ "new entity ID"
    end

    test "has required source_entity_id parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert "source_entity_id" in schema.required
      assert Map.has_key?(schema.properties, "source_entity_id")
      assert schema.properties["source_entity_id"].type == "string"
    end

    test "has optional new_entity_id parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "new_entity_id")
      assert schema.properties["new_entity_id"].type == "string"
      # Not required - auto-generated if not provided
      refute "new_entity_id" in schema.required
    end

    test "has event_types array parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "event_types")
      assert schema.properties["event_types"].type == "array"
    end

    test "has reset_timestamps parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "reset_timestamps")
      assert schema.properties["reset_timestamps"].type == "boolean"
    end

    test "has sanitize_pii parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "sanitize_pii")
      assert schema.properties["sanitize_pii"].type == "boolean"
    end

    test "has dry_run parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "dry_run")
    end
  end

  # ============================================================================
  # merge_entities Tool Definition Tests
  # ============================================================================

  describe "merge_entities tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "merge_entities"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "merge_entities"
    end

    test "has description with usage guidance", %{tool: tool} do
      description = tool.description

      assert description =~ "When to use this tool"
      assert description =~ "Common patterns"
    end

    test "description mentions duplicate merging", %{tool: tool} do
      assert tool.description =~ "duplicate"
      assert tool.description =~ "Merg" or tool.description =~ "merg"
    end

    test "description explains merge behavior", %{tool: tool} do
      description = tool.description

      assert description =~ "combined"
      assert description =~ "chronological" or description =~ "timestamp"
    end

    test "has required source_entity_ids, target_entity_id, and reason parameters", %{tool: tool} do
      schema = tool.inputSchema
      assert "source_entity_ids" in schema.required
      assert "target_entity_id" in schema.required
      assert "reason" in schema.required
    end

    test "source_entity_ids is an array", %{tool: tool} do
      schema = tool.inputSchema
      assert schema.properties["source_entity_ids"].type == "array"
    end

    test "has archive_sources parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "archive_sources")
      assert schema.properties["archive_sources"].type == "boolean"
    end

    test "has delete_sources parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "delete_sources")
      assert schema.properties["delete_sources"].type == "boolean"
    end

    test "has dry_run parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "dry_run")
    end
  end

  # ============================================================================
  # split_entity Tool Definition Tests
  # ============================================================================

  describe "split_entity tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "split_entity"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "split_entity"
    end

    test "has description with usage guidance", %{tool: tool} do
      description = tool.description

      assert description =~ "When to use this tool"
      assert description =~ "Common patterns"
    end

    test "description mentions partitioning and domain-specific entities", %{tool: tool} do
      assert tool.description =~ "Partition" or tool.description =~ "partition"
      assert tool.description =~ "domain"
    end

    test "description explains split criteria", %{tool: tool} do
      description = tool.description

      assert description =~ "event type" or description =~ "event_type"
      assert description =~ "time"
    end

    test "has required source_entity_id, splits, and reason parameters", %{tool: tool} do
      schema = tool.inputSchema
      assert "source_entity_id" in schema.required
      assert "splits" in schema.required
      assert "reason" in schema.required
    end

    test "splits is an array of objects with proper structure", %{tool: tool} do
      schema = tool.inputSchema
      assert schema.properties["splits"].type == "array"
      assert schema.properties["splits"].items.type == "object"

      # Check split object has expected properties
      split_props = schema.properties["splits"].items.properties
      assert Map.has_key?(split_props, "new_entity_id")
      assert Map.has_key?(split_props, "event_types")
      assert Map.has_key?(split_props, "since")
      assert Map.has_key?(split_props, "until")
    end

    test "split item requires new_entity_id", %{tool: tool} do
      schema = tool.inputSchema
      assert schema.properties["splits"].items.required == ["new_entity_id"]
    end

    test "has archive_split_events parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "archive_split_events")
      assert schema.properties["archive_split_events"].type == "boolean"
    end

    test "has delete_split_events parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "delete_split_events")
      assert schema.properties["delete_split_events"].type == "boolean"
    end

    test "has dry_run parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "dry_run")
    end
  end

  # ============================================================================
  # Description Quality Tests - All Tools Have Common Elements
  # ============================================================================

  describe "tool description quality" do
    @event_management_tools [
      "delete_events",
      "archive_events",
      "restore_events",
      "export_events",
      "import_events",
      "clone_entity",
      "merge_entities",
      "split_entity"
    ]

    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      {:ok, tools: tools}
    end

    test "all tools have descriptions with 'When to use this tool' section", %{tools: tools} do
      for tool_name <- @event_management_tools do
        tool = Enum.find(tools, &(&1.name == tool_name))

        assert tool.description =~ "When to use this tool",
               "#{tool_name} missing 'When to use this tool' section"
      end
    end

    test "all tools have descriptions with 'Common patterns' section", %{tools: tools} do
      for tool_name <- @event_management_tools do
        tool = Enum.find(tools, &(&1.name == tool_name))

        assert tool.description =~ "Common patterns",
               "#{tool_name} missing 'Common patterns' section"
      end
    end

    test "lifecycle tools mention audit trail", %{tools: tools} do
      lifecycle_tools = ["delete_events", "archive_events", "restore_events"]

      for tool_name <- lifecycle_tools do
        tool = Enum.find(tools, &(&1.name == tool_name))

        assert tool.description =~ "audit",
               "#{tool_name} should mention audit trail"
      end
    end

    test "tools with dry_run mention preview capability", %{tools: tools} do
      dry_run_tools = [
        "delete_events",
        "archive_events",
        "restore_events",
        "clone_entity",
        "merge_entities",
        "split_entity"
      ]

      for tool_name <- dry_run_tools do
        tool = Enum.find(tools, &(&1.name == tool_name))

        assert tool.description =~ "dry_run" or tool.description =~ "preview",
               "#{tool_name} should mention dry_run/preview"
      end
    end
  end

  # ============================================================================
  # Decision Tree Integration Tests
  # ============================================================================

  describe "event management tools decision tree" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      {:ok, tools: tools}
    end

    test "delete_events mentions restore_events for recovery", %{tools: tools} do
      tool = Enum.find(tools, &(&1.name == "delete_events"))
      assert tool.description =~ "restore_events"
    end

    test "archive_events mentions restore_events for recovery", %{tools: tools} do
      tool = Enum.find(tools, &(&1.name == "archive_events"))
      assert tool.description =~ "restore_events"
    end

    test "export_events mentions formats for different use cases", %{tools: tools} do
      tool = Enum.find(tools, &(&1.name == "export_events"))
      description = tool.description

      # Should explain when to use different formats
      assert description =~ "JSON"
      assert description =~ "analytics" or description =~ "Parquet"
    end

    test "clone_entity mentions PII considerations", %{tools: tools} do
      tool = Enum.find(tools, &(&1.name == "clone_entity"))
      assert tool.description =~ "PII" or tool.description =~ "privacy"
    end

    test "merge_entities mentions projection impact considerations", %{tools: tools} do
      tool = Enum.find(tools, &(&1.name == "merge_entities"))
      assert tool.description =~ "projection" or tool.description =~ "read model"
    end

    test "split_entity mentions projection impact considerations", %{tools: tools} do
      tool = Enum.find(tools, &(&1.name == "split_entity"))
      assert tool.description =~ "projection" or tool.description =~ "read model"
    end
  end
end
