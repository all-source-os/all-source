defmodule McpServerElixir.Protocol.McpToolsLatestStateTest do
  use ExUnit.Case, async: true

  alias McpServerElixir.Protocol.McpTools

  @moduletag :mcp_tools_latest_state

  defmodule ViewsBackend do
    @moduledoc false
    # Echoes its arguments back so a test can see what the handler forwarded.
    def get_latest_state(view, key) do
      {:ok,
       %{
         "view" => view,
         "key" => key,
         "state" => %{"ok" => 1430},
         "version" => 57,
         "revision" => 61,
         "ready" => true
       }}
    end

    def list_latest(view, params) do
      {:ok,
       %{
         "view" => view,
         "params" => params,
         "rows" => [%{"key" => "probe-a:2026-10-26", "state" => %{}, "version" => 40}],
         "next" => nil,
         "revision" => 61
       }}
    end

    def list_changes(view, params) do
      {:ok,
       %{
         "view" => view,
         "params" => params,
         "changes" => [%{"key" => "probe-z:2026-10-20", "version" => 62, "state" => nil}],
         "revision" => 62,
         "caught_up" => true
       }}
    end
  end

  defmodule FailingBackend do
    @moduledoc false
    def get_latest_state(_view, _key), do: {:error, "No row \"k\" in view \"v\""}
    def list_latest(_view, _params), do: {:error, "HTTP 403"}
    def list_changes(_view, _params), do: {:error, "View was rebuilt (generation 3)"}
  end

  defmodule NoViewsBackend do
    @moduledoc false
    def query_events(_params), do: {:ok, %{"events" => [], "count" => 0}}
  end

  defp state(backend) do
    %{backend: backend, read_only: true, control_plane_enabled: false, system_admin: false}
  end

  defp text(content), do: Enum.map_join(content, "\n", & &1.text)

  describe "tool definitions" do
    setup do
      tools = McpTools.list_tools(%{read_only: true})
      {:ok, tools: Map.new(tools, &{&1.name, &1})}
    end

    test "all three are listed in read-only mode", %{tools: tools} do
      for name <- ["get_latest_state", "list_latest", "list_changes"] do
        assert Map.has_key?(tools, name), "#{name} missing from tools/list"
      end
    end

    test "get_latest_state requires view and key", %{tools: tools} do
      assert tools["get_latest_state"].inputSchema.required == ["view", "key"]
    end

    test "list_latest requires only the view and takes a cursor", %{tools: tools} do
      schema = tools["list_latest"].inputSchema
      assert schema.required == ["view"]
      assert Map.has_key?(schema.properties, "prefix")
      assert Map.has_key?(schema.properties, "cursor")
      assert Map.has_key?(schema.properties, "limit")
    end

    test "list_changes requires view and after", %{tools: tools} do
      assert tools["list_changes"].inputSchema.required == ["view", "after"]
    end

    test "descriptions carry the agent guidance sections", %{tools: tools} do
      for name <- ["get_latest_state", "list_latest", "list_changes"] do
        assert tools[name].description =~ "When to use this tool"
        assert tools[name].description =~ "Decision guide"
      end
    end
  end

  describe "get_latest_state" do
    test "forwards view and key and reports version and readiness" do
      args = %{"view" => "ops_status", "key" => "probe-a:2026-10-27", "format" => "json"}

      assert {:ok, %{content: content}} =
               McpTools.call_tool("get_latest_state", args, state(ViewsBackend))

      out = text(content)
      assert out =~ "probe-a:2026-10-27"
      assert out =~ "ops_status"
      assert out =~ "Version: 57"
      assert out =~ "Ready: true"
    end

    test "a backend error is reported with the reason" do
      assert {:error, message} =
               McpTools.call_tool(
                 "get_latest_state",
                 %{"view" => "v", "key" => "k"},
                 state(FailingBackend)
               )

      assert message =~ "Failed to get latest state"
      assert message =~ "No row"
    end

    test "a missing key argument is an error, not a crash" do
      assert {:error, message} =
               McpTools.call_tool("get_latest_state", %{"view" => "v"}, state(ViewsBackend))

      assert message =~ ~s("key")
    end
  end

  describe "list_latest" do
    test "maps cursor to the contract's after parameter" do
      args = %{
        "view" => "ops_status",
        "prefix" => "probe-a:",
        "cursor" => "c1",
        "limit" => 5,
        "format" => "json"
      }

      assert {:ok, %{content: content}} =
               McpTools.call_tool("list_latest", args, state(ViewsBackend))

      out = text(content)
      assert out =~ "1 rows"
      assert out =~ "Revision: 61"
      assert out =~ "last page"
      assert out =~ ~s("after":"c1") or out =~ ~s("after": "c1")
      refute out =~ ~s("cursor")
    end

    test "a backend error is reported with the reason" do
      assert {:error, message} =
               McpTools.call_tool("list_latest", %{"view" => "v"}, state(FailingBackend))

      assert message =~ "Failed to list latest state"
      assert message =~ "403"
    end
  end

  describe "list_changes" do
    test "forwards after and limit and reports the next revision" do
      args = %{"view" => "ops_status", "after" => 61, "limit" => 10, "format" => "json"}

      assert {:ok, %{content: content}} =
               McpTools.call_tool("list_changes", args, state(ViewsBackend))

      out = text(content)
      assert out =~ "1 changes"
      assert out =~ "after revision 61"
      assert out =~ "Next revision: 62"
      assert out =~ "Caught up: true"
    end

    test "after is required" do
      assert {:error, message} =
               McpTools.call_tool("list_changes", %{"view" => "v"}, state(ViewsBackend))

      assert message =~ "after"
    end

    test "a view reset reaches the caller" do
      assert {:error, message} =
               McpTools.call_tool(
                 "list_changes",
                 %{"view" => "v", "after" => 1},
                 state(FailingBackend)
               )

      assert message =~ "rebuilt"
    end
  end

  describe "a backend without views" do
    for {tool, args} <- [
          {"get_latest_state", %{"view" => "v", "key" => "k"}},
          {"list_latest", %{"view" => "v"}},
          {"list_changes", %{"view" => "v", "after" => 0}}
        ] do
      test "#{tool} says the backend does not serve views" do
        args = unquote(Macro.escape(args))

        assert {:error, message} = McpTools.call_tool(unquote(tool), args, state(NoViewsBackend))

        assert message =~ "serves views"
      end
    end
  end
end
