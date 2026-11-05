defmodule McpServerElixir.Protocol.McpToolsContextTest do
  use ExUnit.Case, async: false

  alias McpServerElixir.Context.ConversationContext
  alias McpServerElixir.Protocol.McpTools

  @moduletag :mcp_tools_context

  setup do
    # Own the context rather than sharing the application singleton. Sharing it
    # made this file fail with "no process" whenever anything else brought the
    # app supervisor down — a failure in a process the test neither starts nor
    # controls, which is why it flaked under load and never in isolation (#253).
    name = :"conversation_context_#{System.unique_integer([:positive])}"

    pid =
      start_supervised!(
        Supervisor.child_spec(
          {ConversationContext, name: name, timeout_ms: :timer.seconds(60)},
          id: name
        )
      )

    previous = Application.get_env(:mcp_server_elixir, :conversation_context_server)
    Application.put_env(:mcp_server_elixir, :conversation_context_server, name)

    on_exit(fn ->
      case previous do
        nil -> Application.delete_env(:mcp_server_elixir, :conversation_context_server)
        value -> Application.put_env(:mcp_server_elixir, :conversation_context_server, value)
      end
    end)

    {:ok, pid: pid, server: name}
  end

  describe "list_tools/0" do
    test "includes conversation context tools" do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool_names = Enum.map(tools, & &1.name)

      assert "start_session" in tool_names
      assert "refine_query" in tool_names
      assert "get_session_context" in tool_names
    end

    test "returns 64 tools total" do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      assert length(tools) == 64
    end
  end

  describe "start_session tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "start_session"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "start_session"
    end

    test "has description with agent guidance", %{tool: tool} do
      description = tool.description

      assert description =~ "When to use this tool"
      assert description =~ "Common patterns"
      assert description =~ "Performance tips"
      assert description =~ "Decision guide"
    end

    test "has required session_id parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert schema.required == ["session_id"]
      assert Map.has_key?(schema.properties, "session_id")
    end

    test "has optional context parameters", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "entity_id")
      assert Map.has_key?(schema.properties, "entity_type")
      assert Map.has_key?(schema.properties, "event_type")
      assert Map.has_key?(schema.properties, "since")
      assert Map.has_key?(schema.properties, "until")
      assert Map.has_key?(schema.properties, "semantic_query")
    end
  end

  describe "refine_query tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "refine_query"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "refine_query"
    end

    test "has description explaining merge behavior", %{tool: tool} do
      description = tool.description

      assert description =~ "Merge behavior"
      assert description =~ "Accumulates"
      assert description =~ "Replaces"
    end

    test "has required session_id parameter", %{tool: tool} do
      schema = tool.inputSchema
      assert schema.required == ["session_id"]
    end

    test "has replace_entity_ids option", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "replace_entity_ids")
      assert schema.properties["replace_entity_ids"].type == "boolean"
    end
  end

  describe "get_session_context tool definition" do
    setup do
      tools = McpTools.list_tools(%{control_plane_enabled: true})
      tool = Enum.find(tools, &(&1.name == "get_session_context"))
      {:ok, tool: tool}
    end

    test "has correct name", %{tool: tool} do
      assert tool.name == "get_session_context"
    end

    test "has include_history option", %{tool: tool} do
      schema = tool.inputSchema
      assert Map.has_key?(schema.properties, "include_history")
      assert schema.properties["include_history"].type == "boolean"
    end
  end

  describe "handle_start_session/3" do
    test "creates a new session" do
      session_id = "test-start-#{:erlang.unique_integer([:positive])}"
      args = %{"session_id" => session_id}

      {:ok, result} = McpTools.handle_start_session(args, %{}, nil)

      assert [%{type: "text", text: text}] = result.content
      assert text =~ "Session Started"
      assert text =~ session_id
    end

    test "initializes with provided context" do
      session_id = "test-init-#{:erlang.unique_integer([:positive])}"

      args = %{
        "session_id" => session_id,
        "entity_id" => "user-123",
        "since" => "2024-01-01"
      }

      {:ok, result} = McpTools.handle_start_session(args, %{}, nil)

      assert [%{type: "text", text: text}] = result.content
      assert text =~ "Initial Context"
      assert text =~ "user-123"
      assert text =~ "2024-01-01"
    end

    test "shows empty context message for fresh session" do
      session_id = "test-empty-#{:erlang.unique_integer([:positive])}"
      args = %{"session_id" => session_id}

      {:ok, result} = McpTools.handle_start_session(args, %{}, nil)

      assert [%{type: "text", text: text}] = result.content
      assert text =~ "No initial context" or text =~ "empty"
    end
  end

  describe "handle_refine_query/3" do
    test "refines an existing session" do
      session_id = "test-refine-#{:erlang.unique_integer([:positive])}"

      # Create session first
      {:ok, _} = McpTools.handle_start_session(%{"session_id" => session_id}, %{}, nil)

      # Refine it
      args = %{
        "session_id" => session_id,
        "entity_id" => "user-456",
        "filters" => %{"tier" => "premium"}
      }

      {:ok, result} = McpTools.handle_refine_query(args, %{}, nil)

      assert [%{type: "text", text: text}] = result.content
      assert text =~ "Query Refined"
      assert text =~ "user-456"
      assert text =~ "premium"
    end

    test "returns error when no refinement provided" do
      session_id = "test-no-refine-#{:erlang.unique_integer([:positive])}"
      {:ok, _} = McpTools.handle_start_session(%{"session_id" => session_id}, %{}, nil)

      args = %{"session_id" => session_id}

      assert {:error, error_msg} = McpTools.handle_refine_query(args, %{}, nil)
      assert error_msg =~ "No refinement parameters"
    end

    test "accumulates multiple refinements" do
      session_id = "test-multi-refine-#{:erlang.unique_integer([:positive])}"
      {:ok, _} = McpTools.handle_start_session(%{"session_id" => session_id}, %{}, nil)

      # First refinement
      {:ok, _} =
        McpTools.handle_refine_query(
          %{"session_id" => session_id, "entity_id" => "user-1"},
          %{},
          nil
        )

      # Second refinement
      {:ok, result} =
        McpTools.handle_refine_query(
          %{"session_id" => session_id, "entity_id" => "user-2"},
          %{},
          nil
        )

      assert [%{type: "text", text: text}] = result.content
      # Both entities should be in the accumulated context
      assert text =~ "user-1" or text =~ "user-2"
    end
  end

  describe "handle_get_session_context/3" do
    test "returns session context" do
      session_id = "test-context-#{:erlang.unique_integer([:positive])}"

      # Create and populate session
      {:ok, _} =
        McpTools.handle_start_session(
          %{"session_id" => session_id, "entity_id" => "user-789", "since" => "2024-06-01"},
          %{},
          nil
        )

      args = %{"session_id" => session_id}
      {:ok, result} = McpTools.handle_get_session_context(args, %{}, nil)

      assert [%{type: "text", text: text}] = result.content
      assert text =~ "Session Context"
      assert text =~ session_id
      assert text =~ "user-789"
      assert text =~ "2024-06-01"
    end

    test "returns error for nonexistent session" do
      args = %{"session_id" => "nonexistent-session-xyz"}
      assert {:error, error_msg} = McpTools.handle_get_session_context(args, %{}, nil)
      assert error_msg =~ "not found"
    end

    test "includes query history when requested" do
      session_id = "test-history-#{:erlang.unique_integer([:positive])}"

      {:ok, _} = McpTools.handle_start_session(%{"session_id" => session_id}, %{}, nil)

      {:ok, _} =
        McpTools.handle_refine_query(
          %{"session_id" => session_id, "entity_id" => "user-1"},
          %{},
          nil
        )

      args = %{"session_id" => session_id, "include_history" => true}
      {:ok, result} = McpTools.handle_get_session_context(args, %{}, nil)

      assert [%{type: "text", text: text}] = result.content
      assert text =~ "Query History"
    end
  end

  describe "multi-turn workflow integration" do
    test "complete workflow: start -> refine -> refine -> get context" do
      session_id = "workflow-#{:erlang.unique_integer([:positive])}"

      # Turn 1: Start session with users created yesterday
      {:ok, _} =
        McpTools.handle_start_session(
          %{
            "session_id" => session_id,
            "entity_type" => "user",
            "since" => "yesterday"
          },
          %{},
          nil
        )

      # Turn 2: Filter to premium tier
      {:ok, _} =
        McpTools.handle_refine_query(
          %{"session_id" => session_id, "filters" => %{"tier" => "premium"}},
          %{},
          nil
        )

      # Turn 3: Add specific event type
      {:ok, _} =
        McpTools.handle_refine_query(
          %{"session_id" => session_id, "event_type" => "purchase.completed"},
          %{},
          nil
        )

      # Get final context
      {:ok, result} =
        McpTools.handle_get_session_context(
          %{"session_id" => session_id, "include_history" => true},
          %{},
          nil
        )

      assert [%{type: "text", text: text}] = result.content

      # Verify all context accumulated
      assert text =~ "yesterday"
      assert text =~ "premium"
      assert text =~ "purchase.completed"
      assert text =~ "Query History"
    end
  end
end
