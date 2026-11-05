defmodule McpServerElixir.Protocol.McpToolsAdvertisedTest do
  @moduledoc """
  Every tool `tools/list` advertises must dispatch to a handler that does real
  work. A tool whose only answer is a canned "not implemented" message is a stub
  and must be neither listed nor dispatched.

  Each advertised tool is called once with empty arguments. Core is a probe
  backend that answers every callback with an error and the Control Plane is a
  local socket that answers 400, so nothing a tool attempts can change real data.
  A stub ignores its arguments, so empty arguments are enough to reach its text.
  """
  use ExUnit.Case, async: false

  @moduletag :capture_log

  alias McpServerElixir.Infrastructure.{ControlPlaneClient, CoreBackend}
  alias McpServerElixir.Protocol.McpTools

  @stub_text ~r/not\s+(yet\s+)?(implemented|available)|coming soon/i

  defmodule ProbeBackend do
    @moduledoc false
    @behaviour McpServerElixir.Infrastructure.CoreBackend

    for {name, arity} <- CoreBackend.behaviour_info(:callbacks),
        name != :unsupported_tools do
      args = for i <- 1..arity//1, do: Macro.var(:"_arg#{i}", nil)
      @impl true
      def unquote(name)(unquote_splicing(args)), do: {:error, "probe backend: Core not reached"}
    end
  end

  defmodule NarrowBackend do
    @moduledoc false
    def unsupported_tools, do: ["partition_info", "audit_log"]
    def partition_info(_params), do: {:ok, %{"partitions" => []}}
  end

  @modes [
    %{read_only: false, control_plane_enabled: true, system_admin: true},
    %{read_only: false, control_plane_enabled: false, system_admin: false},
    %{read_only: true, control_plane_enabled: true, system_admin: false}
  ]

  setup do
    original = Application.get_env(:mcp_server_elixir, :control_url)
    on_exit(fn -> Application.put_env(:mcp_server_elixir, :control_url, original) end)
    start_control_plane()
    :ok
  end

  defp start_control_plane do
    {:ok, listen} =
      :gen_tcp.listen(0, [:binary, packet: :raw, active: false, reuseaddr: true, ip: {127, 0, 0, 1}])

    {:ok, port} = :inet.port(listen)
    spawn_link(fn -> refuse_loop(listen) end)
    Application.put_env(:mcp_server_elixir, :control_url, "http://127.0.0.1:#{port}")
  end

  defp refuse_loop(listen) do
    case :gen_tcp.accept(listen) do
      {:ok, sock} ->
        spawn(fn ->
          _ = :gen_tcp.recv(sock, 0, 2_000)
          payload = ~s({"error":"probe control plane"})

          :gen_tcp.send(sock, [
            "HTTP/1.1 400 X\r\ncontent-type: application/json\r\n",
            "content-length: #{byte_size(payload)}\r\n\r\n",
            payload
          ])

          :gen_tcp.close(sock)
        end)

        refuse_loop(listen)

      _ ->
        :ok
    end
  end

  defp state(mode, backend \\ ProbeBackend) do
    Map.merge(mode, %{backend: backend, control_client: ControlPlaneClient.new()})
  end

  # Placeholders for each required argument. Booleans are false, so a tool
  # gated on `confirm` stays in its non-applying path.
  defp required_args(%{inputSchema: schema}) do
    properties = Map.get(schema, :properties, %{})

    for name <- Map.get(schema, :required, []), into: %{} do
      {name, placeholder(get_in(properties, [name, :type]))}
    end
  end

  defp placeholder("string"), do: "probe"
  defp placeholder("number"), do: 1
  defp placeholder("integer"), do: 1
  defp placeholder("boolean"), do: false
  defp placeholder("array"), do: []
  defp placeholder("object"), do: %{}
  defp placeholder(_type), do: "probe"

  defp response_text({:ok, %{content: content}}),
    do: Enum.map_join(content, "\n", &Map.get(&1, :text, ""))

  defp response_text({:ok, other}), do: inspect(other)
  defp response_text({:error, reason}) when is_binary(reason), do: reason
  defp response_text({:error, reason}), do: inspect(reason)

  describe "every advertised tool" do
    for mode <- @modes do
      @mode mode
      test "dispatches to a real handler, never a stub (#{inspect(mode)})" do
        tools = McpTools.list_tools(Map.put(@mode, :backend, ProbeBackend))
        assert tools != []

        stubs =
          for tool <- tools, args <- [%{}, required_args(tool)] do
            {tool.name, args, response_text(McpTools.call_tool(tool.name, args, state(@mode)))}
          end
          |> Enum.filter(fn {_name, _args, text} ->
            text =~ "Unknown tool" or text =~ @stub_text
          end)

        assert stubs == [], "advertised tools answering as stubs: #{inspect(stubs, pretty: true)}"
      end
    end
  end

  describe "the stub detector" do
    test "recognises the canned answers stubs gave" do
      for text <- [
            "Backup List — Not Yet Available",
            "Backup API is not yet implemented in AllSource Core.",
            "This feature is coming soon"
          ] do
        assert text =~ @stub_text
      end

      refute "probe backend: Core not reached" =~ @stub_text
    end
  end

  describe "the Core backends" do
    # The probe backend above stands in for Core, so a canned answer living in
    # a real backend is checked in its source instead.
    test "carry no canned not-implemented answer" do
      root = Path.expand("../../../lib/mcp_server_elixir/infrastructure", __DIR__)

      stubs =
        for file <- ["core_client.ex", "core_embedded.ex"],
            lines = root |> Path.join(file) |> File.read!() |> String.split("\n"),
            {line, n} <- Enum.with_index(lines, 1),
            line =~ @stub_text,
            do: "#{file}:#{n}: #{String.trim(line)}"

      assert stubs == [], "backend stubs: #{inspect(stubs, pretty: true)}"
    end
  end

  describe "backup tools" do
    test "are neither advertised nor dispatched in any mode" do
      for mode <- @modes, name <- ["backup_create", "backup_restore", "backup_list"] do
        names = McpTools.list_tools(mode) |> Enum.map(& &1.name)
        refute name in names
        assert {:error, "Unknown tool: " <> ^name} = McpTools.call_tool(name, %{}, state(mode))
      end
    end
  end

  describe "a backend's unsupported tools" do
    test "are left out of the list" do
      mode = hd(@modes)
      names = McpTools.list_tools(Map.put(mode, :backend, NarrowBackend)) |> Enum.map(& &1.name)

      refute "partition_info" in names
      refute "audit_log" in names
      assert "wal_status" in names
    end

    test "are refused at dispatch even though the backend has the function" do
      st = state(hd(@modes), NarrowBackend)

      assert {:error, "Unknown tool: partition_info"} =
               McpTools.call_tool("partition_info", %{}, st)
    end

    test "do not affect a backend that reports none" do
      mode = hd(@modes)

      assert McpTools.list_tools(Map.put(mode, :backend, ProbeBackend)) ==
               McpTools.list_tools(mode)
    end
  end
end
