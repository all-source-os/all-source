defmodule McpServerElixir.Protocol.McpToolsAnalyticsTest do
  use ExUnit.Case, async: true

  alias McpServerElixir.Protocol.McpTools

  @moduletag :mcp_tools_analytics

  # ============================================================================
  # Tool List Tests
  # ============================================================================

  describe "list_tools/0" do
    test "includes all 8 analytics tools" do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool_names = Enum.map(tools, & &1.name)

      assert "cohort_analysis" in tool_names
      assert "correlation_analysis" in tool_names
      assert "forecast_events" in tool_names
      assert "segment_analysis" in tool_names
      assert "path_analysis" in tool_names
      assert "attribution_analysis" in tool_names
      assert "churn_prediction" in tool_names
      assert "ltv_calculation" in tool_names
    end

    test "returns 64 tools total" do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      assert length(tools) == 64
    end
  end

  # ============================================================================
  # cohort_analysis Tool Definition Tests
  # ============================================================================

  describe "cohort_analysis tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "cohort_analysis"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "cohort_analysis"
    end

    test "has description with agent guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "What it actually returns"
    end

    test "has honest description about limitations", %{tool: tool} do
      assert tool.description =~ "NOT"
      assert tool.description =~ "frequency"
    end

    test "has required event_type parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert "event_type" in schema.required
      assert schema.properties["event_type"].type == "string"
    end

    test "has granularity parameter with correct enum", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "granularity")
      assert schema.properties["granularity"].enum == ["day", "week", "month"]
    end

    test "has periods parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "periods")
      assert schema.properties["periods"].type == "number"
    end

    test "has optional cohort_event_type parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "cohort_event_type")
      assert schema.properties["cohort_event_type"].type == "string"
    end

    test "has time range parameters", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "since")
      assert Map.has_key?(schema.properties, "until")
    end
  end

  # ============================================================================
  # correlation_analysis Tool Definition Tests
  # ============================================================================

  describe "correlation_analysis tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "correlation_analysis"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "correlation_analysis"
    end

    test "has description with agent guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "Methodology"
    end

    test "has honest description about methodology", %{tool: tool} do
      assert tool.description =~ "co-occurrence"
      assert tool.description =~ "not a statistical correlation coefficient"
    end

    test "has required event_type_a parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert "event_type_a" in schema.required
      assert schema.properties["event_type_a"].type == "string"
    end

    test "has required event_type_b parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert "event_type_b" in schema.required
      assert schema.properties["event_type_b"].type == "string"
    end

    test "has optional time_window parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "time_window")
      assert schema.properties["time_window"].type == "string"
    end
  end

  # ============================================================================
  # forecast_events Tool Definition Tests
  # ============================================================================

  describe "forecast_events tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "forecast_events"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "forecast_events"
    end

    test "has description with agent guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "What it actually computes"
    end

    test "has honest description about limitations", %{tool: tool} do
      assert tool.description =~ "basic linear regression"
      assert tool.description =~ "Limitations"
    end

    test "has granularity parameter with correct enum", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "granularity")
      assert schema.properties["granularity"].enum == ["hour", "day", "week", "month"]
    end

    test "has horizon parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "horizon")
      assert schema.properties["horizon"].type == "number"
    end

    test "has history_periods parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "history_periods")
      assert schema.properties["history_periods"].type == "number"
    end

    test "has no required parameters", %{tool: tool} do
      schema = tool.inputSchema
      refute Map.has_key?(schema, :required)
    end
  end

  # ============================================================================
  # segment_analysis Tool Definition Tests
  # ============================================================================

  describe "segment_analysis tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "segment_analysis"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "segment_analysis"
    end

    test "has description with agent guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "What it actually returns"
    end

    test "has honest description about limitations", %{tool: tool} do
      assert tool.description =~ "NOT"
      assert tool.description =~ "summary statistics"
    end

    test "has required event_type parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert "event_type" in schema.required
      assert schema.properties["event_type"].type == "string"
    end

    test "has metric parameter with correct enum", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "metric")

      assert schema.properties["metric"].enum == [
               "frequency",
               "recency",
               "monetary_value",
               "rfm"
             ]
    end

    test "has segments parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "segments")
      assert schema.properties["segments"].type == "number"
    end
  end

  # ============================================================================
  # path_analysis Tool Definition Tests
  # ============================================================================

  describe "path_analysis tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "path_analysis"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "path_analysis"
    end

    test "has description with agent guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "What it actually returns"
    end

    test "has honest description about limitations", %{tool: tool} do
      assert tool.description =~ "NOT"
      assert tool.description =~ "funnel"
    end

    test "has steps parameter as array", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "steps")
      assert schema.properties["steps"].type == "array"
    end

    test "has start_event parameter for open path discovery", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "start_event")
      assert schema.properties["start_event"].type == "string"
    end

    test "has depth parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "depth")
      assert schema.properties["depth"].type == "number"
    end

    test "has max_duration parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "max_duration")
      assert schema.properties["max_duration"].type == "string"
    end
  end

  # ============================================================================
  # attribution_analysis Tool Definition Tests
  # ============================================================================

  describe "attribution_analysis tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "attribution_analysis"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "attribution_analysis"
    end

    test "has description with agent guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "What it actually returns"
    end

    test "has honest description about limitations", %{tool: tool} do
      assert tool.description =~ "NOT"
      assert tool.description =~ "attribution"
    end

    test "has required target_event parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert "target_event" in schema.required
      assert schema.properties["target_event"].type == "string"
    end

    test "has model parameter with correct enum", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "model")

      assert schema.properties["model"].enum == [
               "first_touch",
               "last_touch",
               "linear",
               "time_decay"
             ]
    end

    test "has touchpoint_types parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "touchpoint_types")
      assert schema.properties["touchpoint_types"].type == "array"
    end

    test "has lookback parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "lookback")
      assert schema.properties["lookback"].type == "string"
    end
  end

  # ============================================================================
  # churn_prediction Tool Definition Tests
  # ============================================================================

  describe "churn_prediction tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "churn_prediction"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "churn_prediction"
    end

    test "has description with agent guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "What it actually returns"
    end

    test "has honest description about limitations", %{tool: tool} do
      assert tool.description =~ "NOT"
      assert tool.description =~ "churn"
    end

    test "has required activity_event parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert "activity_event" in schema.required
      assert schema.properties["activity_event"].type == "string"
    end

    test "has lookback parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "lookback")
      assert schema.properties["lookback"].type == "string"
    end

    test "has risk_threshold parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "risk_threshold")
      assert schema.properties["risk_threshold"].type == "number"
    end

    test "has include_factors parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "include_factors")
      assert schema.properties["include_factors"].type == "boolean"
    end
  end

  # ============================================================================
  # ltv_calculation Tool Definition Tests
  # ============================================================================

  describe "ltv_calculation tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "ltv_calculation"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "ltv_calculation"
    end

    test "has description with agent guidance", %{tool: tool} do
      assert tool.description =~ "When to use this tool"
      assert tool.description =~ "What it actually returns"
    end

    test "has honest description about limitations", %{tool: tool} do
      assert tool.description =~ "NOT"
      assert tool.description =~ "LTV"
    end

    test "has required value_event parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert "value_event" in schema.required
      assert schema.properties["value_event"].type == "string"
    end

    test "has required value_field parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert "value_field" in schema.required
      assert schema.properties["value_field"].type == "string"
    end

    test "has projection_months parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "projection_months")
      assert schema.properties["projection_months"].type == "number"
    end

    test "has group_by parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "group_by")
      assert schema.properties["group_by"].type == "string"
    end
  end

  # ============================================================================
  # Description Quality Tests - All Analytics Tools
  # ============================================================================

  describe "analytics tool description quality" do
    @analytics_tools [
      "cohort_analysis",
      "correlation_analysis",
      "forecast_events",
      "segment_analysis",
      "path_analysis",
      "attribution_analysis",
      "churn_prediction",
      "ltv_calculation"
    ]

    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      {:ok, tools: tools}
    end

    test "all tools have descriptions with 'When to use this tool' section", %{tools: tools} do
      for tool_name <- @analytics_tools do
        tool = Enum.find(tools, &(&1.name == tool_name))

        assert tool.description =~ "When to use this tool",
               "#{tool_name} missing 'When to use this tool' section"
      end
    end

    test "all tools have honest descriptions about actual capabilities", %{tools: tools} do
      for tool_name <- @analytics_tools do
        tool = Enum.find(tools, &(&1.name == tool_name))

        assert tool.description =~ "actually" or tool.description =~ "Important" or
                 tool.description =~ "Note:",
               "#{tool_name} missing honest description of actual capabilities"
      end
    end

    test "all analytics tools have tenant_id filter", %{tools: tools} do
      for tool_name <- @analytics_tools do
        tool = Enum.find(tools, &(&1.name == tool_name))
        schema = tool.inputSchema

        assert Map.has_key?(schema.properties, "tenant_id"),
               "#{tool_name} missing tenant_id parameter"
      end
    end
  end
end
