{:ok, _} = Application.ensure_all_started(:mcp_server_elixir)

unless Process.whereis(McpServerElixir.Context.ConversationContext) do
  raise "ConversationContext must stay supervised throughout the test suite"
end

ExUnit.start(exclude: [:integration, :embedded])
