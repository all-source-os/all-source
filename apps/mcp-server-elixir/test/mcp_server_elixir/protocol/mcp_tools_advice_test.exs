defmodule McpServerElixir.Protocol.McpToolsAdviceTest do
  use ExUnit.Case, async: true

  alias McpServerElixir.Protocol.McpTools

  @moduletag :mcp_tools_advice

  describe "list_tools/0" do
    test "includes get_query_advice tool" do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool_names = Enum.map(tools, & &1.name)
      assert "get_query_advice" in tool_names
    end
  end

  describe "get_query_advice tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "get_query_advice"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "get_query_advice"
    end

    test "has description with agent guidance", %{tool: tool} do
      description = tool.description

      # Check for agent guidance sections
      assert description =~ "When to use this tool"
      assert description =~ "Common patterns"
      assert description =~ "How it works"
      assert description =~ "Context keywords"
    end

    test "has required use_case parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert schema.required == ["use_case"]
      assert Map.has_key?(schema.properties, "use_case")
      assert schema.properties["use_case"].type == "string"
    end

    test "use_case has correct enum values", %{tool: tool} do
      schema = tool.inputSchema

      expected_enum = [
        "audit_trail",
        "user_analytics",
        "debugging",
        "compliance",
        "performance_analysis"
      ]

      assert schema.properties["use_case"].enum == expected_enum
    end

    test "has optional context parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "context")
      assert schema.properties["context"].type == "string"
      refute "context" in schema.required
    end
  end

  describe "handle_get_query_advice/2" do
    test "returns advice for audit_trail use case" do
      args = %{"use_case" => "audit_trail"}
      {:ok, result} = McpTools.handle_get_query_advice(args, nil)

      assert [%{type: "text", text: text}] = result.content
      assert text =~ "Audit Trail Investigation"
      # Check for tool recommendations (ToonEncoder formats as table)
      assert text =~ "event_timeline"
      assert text =~ "query_events"
      assert text =~ "reconstruct_state"
    end

    test "returns advice for user_analytics use case" do
      args = %{"use_case" => "user_analytics"}
      {:ok, result} = McpTools.handle_get_query_advice(args, nil)

      assert [%{type: "text", text: text}] = result.content
      assert text =~ "User Analytics"
      assert text =~ "find_patterns"
      assert text =~ "compare_entities"
    end

    test "returns advice for debugging use case" do
      args = %{"use_case" => "debugging"}
      {:ok, result} = McpTools.handle_get_query_advice(args, nil)

      assert [%{type: "text", text: text}] = result.content
      assert text =~ "Debugging"
      assert text =~ "event_timeline"
      # get_cluster_status is included as a recommended tool
      assert text =~ "get_cluster_status" or text =~ "cluster_status"
    end

    test "returns advice for compliance use case" do
      args = %{"use_case" => "compliance"}
      {:ok, result} = McpTools.handle_get_query_advice(args, nil)

      assert [%{type: "text", text: text}] = result.content
      assert text =~ "Compliance"
      assert text =~ "reconstruct_state"
      assert text =~ "analyze_changes"
    end

    test "returns advice for performance_analysis use case" do
      args = %{"use_case" => "performance_analysis"}
      {:ok, result} = McpTools.handle_get_query_advice(args, nil)

      assert [%{type: "text", text: text}] = result.content
      assert text =~ "Performance Analysis"
      assert text =~ "get_stats"
      assert text =~ "get_cluster_status"
    end

    test "includes context in output when provided" do
      args = %{"use_case" => "debugging", "context" => "e-commerce checkout flow"}
      {:ok, result} = McpTools.handle_get_query_advice(args, nil)

      assert [%{type: "text", text: text}] = result.content
      assert text =~ "e-commerce checkout flow"
    end

    test "adds context-specific tips for payment context" do
      args = %{"use_case" => "debugging", "context" => "payment processing"}
      {:ok, result} = McpTools.handle_get_query_advice(args, nil)

      assert [%{type: "text", text: text}] = result.content
      assert text =~ "payment" or text =~ "transaction"
    end

    test "adds context-specific tips for authentication context" do
      args = %{"use_case" => "user_analytics", "context" => "user authentication flow"}
      {:ok, result} = McpTools.handle_get_query_advice(args, nil)

      assert [%{type: "text", text: text}] = result.content
      # Context tips include session or authentication related advice
      assert text =~ "session" or text =~ "Authentication" or text =~ "authentication"
    end

    test "adds context-specific tips for user/customer context" do
      args = %{"use_case" => "audit_trail", "context" => "customer account management"}
      {:ok, result} = McpTools.handle_get_query_advice(args, nil)

      assert [%{type: "text", text: text}] = result.content
      # Context tips include user journey or entity exploration advice
      assert text =~ "explain_entity" or text =~ "journey" or text =~ "account"
    end
  end

  describe "advice content quality" do
    test "audit_trail advice includes event_timeline as top recommendation" do
      args = %{"use_case" => "audit_trail"}
      {:ok, result} = McpTools.handle_get_query_advice(args, nil)

      assert [%{type: "text", text: text}] = result.content
      assert text =~ "event_timeline"
      assert text =~ "RECOMMENDED TOOLS"
      assert text =~ "1. event_timeline"
    end

    test "debugging advice includes cluster_status check" do
      args = %{"use_case" => "debugging"}
      {:ok, result} = McpTools.handle_get_query_advice(args, nil)

      assert [%{type: "text", text: text}] = result.content
      assert text =~ "get_cluster_status"
      assert text =~ "system health" or text =~ "Check system health"
    end

    test "compliance advice includes reconstruct_state for regulatory proof" do
      args = %{"use_case" => "compliance"}
      {:ok, result} = McpTools.handle_get_query_advice(args, nil)

      assert [%{type: "text", text: text}] = result.content
      assert text =~ "reconstruct_state"
      assert text =~ "audit" or text =~ "Compliance"
    end

    test "performance_analysis includes both stats and cluster_status" do
      args = %{"use_case" => "performance_analysis"}
      {:ok, result} = McpTools.handle_get_query_advice(args, nil)

      assert [%{type: "text", text: text}] = result.content
      assert text =~ "get_stats"
      assert text =~ "get_cluster_status"
    end

    test "user_analytics includes pattern discovery tools" do
      args = %{"use_case" => "user_analytics"}
      {:ok, result} = McpTools.handle_get_query_advice(args, nil)

      assert [%{type: "text", text: text}] = result.content
      assert text =~ "find_patterns"
      assert text =~ "sequence"
    end
  end

  describe "tool count" do
    test "list_tools returns 64 tools" do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      assert length(tools) == 64
    end
  end
end
