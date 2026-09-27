defmodule QueryServiceEx.TestSupport.CustomerAgentClaude do
  @moduledoc """
  Opt-in actual Claude Code driver for synthetic customer-review host proof.
  No shell/write/browser tools, other MCP servers or permission bypass. Runs in
  a fresh private directory, with the complete customer skill and a bounded model
  budget. Not a customer install, processing consent or outcome claim.
  """

  alias QueryServiceEx.TestSupport.CustomerAgentConnection

  def run(context, token, binding, mcp_binary, claude_binary) do
    directory = Path.join(context.directory, "claude-host")
    skill = Path.join(directory, ".claude/skills/allsource-customer")
    File.mkdir_p!(Path.dirname(skill))
    File.cp_r!(Path.expand("../../../../skills/allsource-customer", __DIR__), skill)
    config_file = Path.join(directory, "mcp.json")

    connection_file =
      CustomerAgentConnection.write(context, token, binding)

    config = %{
      mcpServers: %{
        allsource_review: %{
          command: mcp_binary,
          args: ["start"],
          env: %{
            "ALLSOURCE_CUSTOMER_REVIEW" => "true",
            "CUSTOMER_REVIEW_CONNECTION_FILE" => connection_file,
            "CORE_API_KEY" => "",
            "ALLSOURCE_CORE_API_KEY" => "",
            "CORE_MODE" => "remote",
            "CORE_WS_ENABLED" => "false"
          }
        }
      }
    }

    File.write!(config_file, Jason.encode!(config))
    File.chmod!(config_file, 0o600)

    prompt = """
    Use the installed allsource-customer skill and read its workflow and human-gate
    references. This is an explicitly authorized synthetic local integration test.
    Check the AllSource customer review connection, then validate this proposal:
    {"schema_version":1,"kind":"event_timeline","projection_name":null,"sources":[]}
    Use the two discovered allsource_review MCP tools, not invented endpoints.
    Report exact resulting states and what remains unavailable. Do not retrieve
    private data, prepare an unimplemented draft, approve anything or use other tools.
    """

    allowed =
      "Skill,Read(./.claude/skills/allsource-customer/**),mcp__allsource_review__allsource_review_context,mcp__allsource_review__allsource_validate_review_proposal"

    args = [
      "-k",
      "5",
      "150",
      claude_binary,
      "--print",
      prompt,
      "--verbose",
      "--output-format",
      "stream-json",
      "--setting-sources",
      "project",
      "--tools",
      "Skill,Read",
      "--allowedTools",
      allowed,
      "--mcp-config",
      config_file,
      "--strict-mcp-config",
      "--permission-mode",
      "dontAsk",
      "--no-chrome",
      "--no-session-persistence",
      "--max-budget-usd",
      "2"
    ]

    # test-hang-allow: gtimeout bounds the CLI and its child process group.
    {output, status} =
      System.cmd(System.find_executable("gtimeout"), args, cd: directory, stderr_to_stdout: true)

    if path = System.get_env("ALLSOURCE_CLAUDE_TRACE") do
      File.write!(path, output)
      File.chmod!(path, 0o600)
    end

    events =
      output
      |> String.split("\n", trim: true)
      |> Enum.flat_map(fn line ->
        case Jason.decode(line) do
          {:ok, event} when is_map(event) -> [event]
          _ -> []
        end
      end)

    %{status: status, events: events, leaked_token: String.contains?(output, token)}
  end

  def tool_calls(events) do
    events |> blocks() |> Enum.filter(&(&1["type"] == "tool_use"))
  end

  def results(events) do
    events
    |> blocks()
    |> Enum.filter(&(&1["type"] == "tool_result"))
    |> Enum.flat_map(fn block ->
      content =
        case block["content"] do
          value when is_binary(value) ->
            [value]

          values when is_list(values) ->
            Enum.flat_map(values, fn
              %{"type" => "text", "text" => text} -> [text]
              _ -> []
            end)

          _ ->
            []
        end

      Enum.flat_map(content, fn text ->
        case Jason.decode(text) do
          {:ok, %{"state" => _} = result} -> [result]
          _ -> []
        end
      end)
    end)
  end

  defp blocks(events), do: Enum.flat_map(events, &(get_in(&1, ["message", "content"]) || []))
end
