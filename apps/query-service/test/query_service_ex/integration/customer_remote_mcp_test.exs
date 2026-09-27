defmodule QueryServiceEx.Integration.CustomerRemoteMCPTest do
  use ExUnit.Case, async: false
  alias QueryServiceEx.TestSupport.CustomerRemoteHTTP, as: Remote
  alias QueryServiceEx.TestSupport.CustomerRemoteMCP, as: MCP
  alias QueryServiceEx.TestSupport.CustomerRemoteWeb
  import QueryServiceEx.TestSupport.CustomerAgentCore, only: [with_core: 2]
  @moduletag :integration
  @moduletag timeout: 90_000
  @moduletag skip:
               is_nil(System.get_env("ALLSOURCE_CORE_BINARY")) or
                 is_nil(System.get_env("ALLSOURCE_CUSTOMER_MCP_BINARY"))

  setup do
    Remote.setup_context()
  end

  test "real Core, Query Service and compiled MCP HTTP release enforce the remote connection lifecycle",
       context do
    with_core(context, fn ->
      Remote.provision()

      MCP.with_mcp(context, fn url ->
        assert {401, _, headers} = MCP.http(url, :get, nil)

        assert Enum.any?(headers, fn {key, value} ->
                 key == "www-authenticate" and String.contains?(value, "oauth-protected-resource")
               end)

        {token, code} = Remote.connect(context)

        assert {200, %{"result" => %{"protocolVersion" => "2025-11-25"}}, _} =
                 MCP.http(
                   url,
                   :post,
                   token,
                   rpc("initialize", %{
                     protocolVersion: "2025-11-25",
                     capabilities: %{},
                     clientInfo: %{name: "synthetic-http-client", version: "1"}
                   })
                 )

        assert {202, nil, _} =
                 MCP.http(url, :post, token, %{
                   jsonrpc: "2.0",
                   method: "notifications/initialized"
                 })

        assert {200, %{"result" => %{"tools" => tools}}, _} =
                 MCP.http(url, :post, token, rpc("tools/list"))

        assert Enum.map(tools, & &1["name"]) == [
                 "allsource_review_context",
                 "allsource_validate_review_proposal"
               ]

        assert {200,
                %{"result" => %{"structuredContent" => %{"state" => "eligibility_verified"}}}, _} =
                 MCP.http(
                   url,
                   :post,
                   token,
                   rpc("tools/call", %{name: "allsource_review_context", arguments: %{}})
                 )

        proposal = %{schema_version: 1, kind: "event_timeline", projection_name: nil, sources: []}

        assert {200,
                %{
                  "result" => %{
                    "structuredContent" => %{
                      "state" => "valid_unresolved",
                      "approved" => false,
                      "persisted" => false
                    }
                  }
                }, _} =
                 MCP.http(
                   url,
                   :post,
                   token,
                   rpc("tools/call", %{
                     name: "allsource_validate_review_proposal",
                     arguments: %{proposal: proposal}
                   })
                 )

        for method <- ["resources/read", "approve_review", "ingest_event"] do
          assert {200, %{"error" => %{"code" => -32_601}}, _} =
                   MCP.http(url, :post, token, rpc(method))
        end

        assert {403, _, _} =
                 MCP.http(url, :post, token, rpc("tools/list"), [{"origin", "https://evil.test"}])

        assert {403, _, _} = MCP.http(url, :post, token, rpc("tools/list"), [], "?token=private")
        assert {405, _, _} = MCP.http(url, :get, token)
        assert {413, _, _} = MCP.http(url, :post, token, String.duplicate("x", 65_537))
        assert {400, _, _} = Remote.token_request(context, Remote.exchange(code))
        assert {401, _, _} = MCP.http(url, :post, token, rpc("tools/list"))
        {fresh, _} = Remote.connect(context)
        assert {200, _, _} = MCP.http(url, :post, fresh, rpc("tools/list"))
        Remote.billing("canceled")
        assert {401, _, _} = MCP.http(url, :post, fresh, rpc("tools/list"))
      end)
    end)
  end

  @tag :browser_fixture
  @tag timeout: 280_000
  @tag skip: System.get_env("ALLSOURCE_REMOTE_BROWSER_FIXTURE") != "1"
  test "opt-in browser fixture for clean consent and same-origin handoff", context do
    with_core(context, fn ->
      Remote.provision()
      # test-hang-allow: owned manual fixture has 240s deadline and local stop route.
      start_supervised!(%{
        id: :remote_browser_fixture,
        start:
          {Bandit, :start_link,
           [
             [
               plug:
                 {QueryServiceEx.Integration.CustomerConnectionsTest.BrowserFixture,
                  [token: Remote.session(), owner: self()]},
               port: 4345,
               ip: {127, 0, 0, 1}
             ]
           ]}
      })

      MCP.with_mcp(
        %{context | query_url: "http://127.0.0.1:4345"},
        fn _ ->
          if System.get_env("ALLSOURCE_REMOTE_WEB_URL"),
            do: CustomerRemoteWeb.verify()

          IO.puts("Synthetic remote browser fixture ready on http://127.0.0.1:4345 (MCP 4346)")

          receive do
            :fixture_stop -> :ok
          after
            240_000 -> :ok
          end
        end,
        4346
      )
    end)
  end

  defp rpc(method, params \\ %{}), do: %{jsonrpc: "2.0", id: 1, method: method, params: params}
end
