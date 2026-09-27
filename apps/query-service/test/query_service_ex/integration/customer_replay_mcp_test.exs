defmodule QueryServiceEx.Integration.CustomerReplayMCPTest do
  use ExUnit.Case, async: false
  alias QueryServiceEx.TestSupport.AgentRunFixture, as: Run
  alias QueryServiceEx.TestSupport.CustomerAgentConnection
  alias QueryServiceEx.TestSupport.CustomerAgentCore, as: Core
  alias QueryServiceEx.TestSupport.CustomerReplayFixture, as: R
  alias QueryServiceEx.TestSupport.MeteredEvidenceFixture, as: F
  @mcp System.get_env("ALLSOURCE_CUSTOMER_MCP_BINARY")
  @moduletag :integration
  @moduletag timeout: 120_000
  @moduletag skip: is_nil(@mcp) or is_nil(System.get_env("ALLSOURCE_CORE_BINARY"))
  setup do: R.setup_context()

  test "compiled MCP prepares and reads actual product decision, never exposing approval",
       context do
    Core.with_core(context, fn ->
      now = System.system_time(:second)
      {grant, _} = R.provision(now)
      input = R.prepare_input(grant, now)

      with_stdio(context, grant, fn port ->
        assert %{"result" => %{"tools" => tools}} = rpc(port, 1, "tools/list", %{})
        assert length(tools) == 5
        refute Enum.any?(tools, &String.contains?(&1["name"], "approve"))
        pending = tool(port, 2, "allsource_prepare_review", input)
        assert pending["state"] == "pending"
        assert pending["decision"] == nil

        args =
          Map.take(pending, ~w(id version digest)) |> Map.put("request_id", F.operation(901, now))

        assert tool(port, 3, "allsource_get_review", args) == pending

        assert %{"error" => %{"code" => -32_602}} =
                 rpc(port, 4, "tools/call", %{
                   "name" => "allsource_prepare_review",
                   "arguments" => Map.put(input, "approved", true)
                 })

        decision =
          Map.merge(args, %{"decision_id" => Run.uuid(902), "request_id" => F.operation(903, now)})

        assert {200, %{"data" => approved}, _} = R.post(context, "approve", grant, decision)
        assert approved["approved_digest"] == pending["digest"]
        result = tool(port, 5, "allsource_get_review_result", args)
        assert result["state"] == "approved"
        assert result["decision"]["digest"] == pending["digest"]
        assert result["replay"]["status"] in ~w(running completed)
        refute inspect(result) =~ "untrusted:"
        Application.put_env(:query_service_ex, :customer_replay_enabled, false)

        assert %{"result" => %{"isError" => true}} =
                 rpc(port, 6, "tools/call", %{
                   "name" => "allsource_get_review",
                   "arguments" => args
                 })
      end)
    end)
  end

  defp with_stdio(context, grant, fun) do
    path = CustomerAgentConnection.write(context, grant.token, F.binding())

    env = %{
      "ALLSOURCE_CUSTOMER_REVIEW" => "true",
      "ALLSOURCE_CUSTOMER_EVIDENCE_REVIEW" => "true",
      "ALLSOURCE_CUSTOMER_REPLAY_REVIEW" => "true",
      "ALLSOURCE_CUSTOMER_REVIEW_HTTP" => "false",
      "CUSTOMER_REVIEW_CONNECTION_FILE" => path,
      "CORE_API_KEY" => "",
      "ALLSOURCE_CORE_API_KEY" => "",
      "CORE_WS_ENABLED" => "false"
    }

    env =
      Enum.map(env, fn {key, value} -> {String.to_charlist(key), String.to_charlist(value)} end)

    # test-hang-allow: owned release with bounded response deadlines and unconditional cleanup.
    port =
      Port.open({:spawn_executable, String.to_charlist(@mcp)}, [
        :binary,
        :exit_status,
        {:line, 150_000},
        {:args, ["start"]},
        {:env, env}
      ])

    try do
      fun.(port)
    after
      case Port.info(port, :os_pid) do
        {:os_pid, pid} ->
          System.cmd("/bin/kill", ["-KILL", to_string(pid)])

          receive do
            {^port, {:exit_status, _}} -> :ok
          after
            5_000 -> flunk("Owned MCP process did not exit")
          end

        nil ->
          :ok
      end
    end
  end

  defp tool(port, id, name, arguments) do
    assert %{
             "result" => %{
               "isError" => false,
               "structuredContent" => data,
               "content" => [%{"text" => text}]
             }
           } = rpc(port, id, "tools/call", %{"name" => name, "arguments" => arguments})

    assert Jason.decode!(text) == data
    data
  end

  defp rpc(port, id, method, params) do
    Port.command(
      port,
      Jason.encode!(%{jsonrpc: "2.0", id: id, method: method, params: params}) <> "\n"
    )

    receive do
      {^port, {:data, {:eol, line}}} -> Jason.decode!(line)
      {^port, {:exit_status, _}} -> flunk("Owned MCP process exited")
    after
      30_000 -> flunk("MCP response deadline exceeded")
    end
  end
end
