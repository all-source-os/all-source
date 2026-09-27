defmodule QueryServiceEx.Integration.CustomerAgentHTTPTest do
  use ExUnit.Case, async: false

  import ExUnit.CaptureLog
  import QueryServiceEx.TestSupport.CustomerAgentCore, only: [with_core: 2]

  alias QueryServiceEx.Infrastructure.Adapters.CustomerAgentGrantStore
  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient
  alias QueryServiceEx.TestSupport.CustomerAgentClaude
  alias QueryServiceEx.TestSupport.CustomerAgentCore

  @moduletag :integration
  @moduletag timeout: 120_000
  @moduletag skip: is_nil(System.get_env("ALLSOURCE_CORE_BINARY"))
  @mcp_binary System.get_env("ALLSOURCE_CUSTOMER_MCP_BINARY")
  @claude_binary System.get_env("ALLSOURCE_CLAUDE_BINARY")
  @binding %{
    "tenant_id" => "http-review-tenant",
    "subject_id" => "oauth:google:123456789",
    "client_id" => "claude-code",
    "resource" => "https://api.example.test/customer-review"
  }
  @proposal %{
    "schema_version" => 1,
    "kind" => "event_timeline",
    "projection_name" => nil,
    "sources" => []
  }

  setup do
    core = CustomerAgentCore.setup_context()
    keys = [:customer_review_enabled, :customer_review_resource]
    previous = Enum.map(keys, &{&1, Application.get_env(:query_service_ex, &1)})
    Application.put_env(:query_service_ex, :customer_review_enabled, true)
    Application.put_env(:query_service_ex, :customer_review_resource, @binding["resource"])

    on_exit(fn ->
      Enum.each(previous, fn
        {key, nil} -> Application.delete_env(:query_service_ex, key)
        {key, value} -> Application.put_env(:query_service_ex, key, value)
      end)

      :ets.delete(:rate_limiter_buckets, "customer-review:admission")
      :ets.delete(:rate_limiter_buckets, "customer-review:" <> @binding["tenant_id"])
    end)

    # test-hang-allow: supervised loopback HTTP server, request deadlines below.
    server =
      start_supervised!({Bandit, plug: QueryServiceExWeb.Endpoint, port: 0, ip: {127, 0, 0, 1}})

    {:ok, {_ip, port}} = ThousandIsland.listener_info(server)
    Map.put(core, :query_url, "http://127.0.0.1:#{port}")
  end

  test "real HTTP path checks grants, bounds input and validates without approving or persisting",
       context do
    with_core(context, fn ->
      issued = provision()
      body = %{"binding" => @binding}

      assert {200,
              %{"data" => %{"state" => "eligibility_verified", "preparation_available" => false}},
              headers} = request(context, "context", issued.token, body)

      assert {"cache-control", "no-store"} in headers

      assert {200, %{"data" => result}, _} =
               request(context, "validate", issued.token, Map.put(body, "proposal", @proposal))

      assert result["state"] == "valid_unresolved"
      assert result["approved"] == false
      assert result["persisted"] == false
      assert "unresolved_source_authority" in result["unknowns"]

      for candidate <- [
            %{},
            Map.put(@proposal, "approved", true),
            Map.put(@proposal, "kind", "execute_query")
          ] do
        assert {422, %{"error" => %{"code" => "invalid_proposal"}}, _} =
                 request(context, "validate", issued.token, Map.put(body, "proposal", candidate))
      end

      assert {403, _, _} = request(context, "context", "not-a-grant", body)

      assert {403, _, _} =
               request(context, "context", CustomerAgentCore.token("admin"), body)

      assert {403, _, _} =
               request(context, "context", issued.token, %{
                 "binding" => Map.put(@binding, "tenant_id", "wrong")
               })

      assert {403, _, _} =
               request(context, "context", issued.token, %{
                 "binding" => Map.put(@binding, "resource", "https://other.example.test/review")
               })

      assert {413, _, _} =
               request(
                 context,
                 "validate",
                 issued.token,
                 Map.put(body, "proposal", String.duplicate("x", 70_000))
               )

      Application.put_env(:query_service_ex, :customer_review_enabled, false)
      assert {403, _, _} = request(context, "context", issued.token, body)
      Application.put_env(:query_service_ex, :customer_review_enabled, true)

      assert :ok =
               CustomerAgentGrantStore.revoke(@binding, issued.id, System.system_time(:second))

      assert {403, _, _} = request(context, "context", issued.token, body)
    end)
  end

  test "request errors do not log proposal values, grant credentials or query strings", context do
    with_core(context, fn ->
      issued = provision()
      marker = "PRIVATE-REVIEW-FIXTURE-DO-NOT-LOG"

      log =
        capture_log(fn ->
          body = %{"binding" => @binding, "proposal" => Map.put(@proposal, "extra", marker)}
          assert {422, _, _} = request(context, "validate", issued.token, body)

          assert {403, _, _} =
                   request(context, "context?secret=" <> marker, issued.token, %{
                     "binding" => @binding
                   })
        end)

      refute log =~ marker
      refute log =~ issued.token
      refute log =~ @binding["subject_id"]
    end)
  end

  @tag skip: is_nil(@mcp_binary)
  test "compiled stdio MCP profile reaches real HTTP/Core and denies broad tools and revoked reconnects",
       context do
    with_core(context, fn ->
      issued = provision()

      with_mcp(context, issued.token, fn port ->
        assert %{"result" => %{"serverInfo" => %{"name" => "allsource-customer-review"}}} =
                 rpc(port, 1, "initialize", %{
                   "protocolVersion" => "2025-06-18",
                   "capabilities" => %{},
                   "clientInfo" => %{"name" => "synthetic-proof", "version" => "1"}
                 })

        assert %{"result" => %{"tools" => tools}} = rpc(port, 2, "tools/list", %{})

        assert Enum.map(tools, & &1["name"]) == [
                 "allsource_review_context",
                 "allsource_validate_review_proposal"
               ]

        assert %{
                 "result" => %{
                   "isError" => false,
                   "structuredContent" => %{"state" => "eligibility_verified"}
                 }
               } =
                 context_response =
                 rpc(port, 3, "tools/call", %{
                   "name" => "allsource_review_context",
                   "arguments" => %{}
                 })

        assert [%{"type" => "text", "text" => context_text}] =
                 context_response["result"]["content"]

        assert Jason.decode!(context_text) == context_response["result"]["structuredContent"]

        assert %{
                 "result" => %{
                   "isError" => false,
                   "structuredContent" => %{"approved" => false, "persisted" => false}
                 }
               } =
                 validation_response =
                 rpc(port, 4, "tools/call", %{
                   "name" => "allsource_validate_review_proposal",
                   "arguments" => %{"proposal" => @proposal}
                 })

        assert [%{"type" => "text", "text" => validation_text}] =
                 validation_response["result"]["content"]

        assert Jason.decode!(validation_text) ==
                 validation_response["result"]["structuredContent"]

        for name <- ["query_events", "replay_events", "create_tenant", "approve_review"] do
          assert %{"error" => %{"code" => -32_602}} =
                   rpc(port, name, "tools/call", %{"name" => name, "arguments" => %{}})
        end

        assert %{"error" => %{"code" => -32_602}} =
                 rpc(port, 5, "tools/call", %{
                   "name" => "allsource_review_context",
                   "arguments" => "bad"
                 })

        assert :ok =
                 CustomerAgentGrantStore.revoke(@binding, issued.id, System.system_time(:second))

        assert %{"result" => %{"isError" => true}} =
                 rpc(port, 6, "tools/call", %{
                   "name" => "allsource_review_context",
                   "arguments" => %{}
                 })
      end)

      with_mcp(context, issued.token, fn port ->
        assert %{"result" => %{"isError" => true}} =
                 rpc(port, 7, "tools/call", %{
                   "name" => "allsource_review_context",
                   "arguments" => %{}
                 })
      end)
    end)
  end

  @tag skip: is_nil(@mcp_binary) or is_nil(@claude_binary)
  @tag timeout: 180_000
  test "actual Claude Code uses installed customer skill and reports unresolved validation",
       context do
    with_core(context, fn ->
      issued = provision()

      proof =
        CustomerAgentClaude.run(context, issued.token, @binding, @mcp_binary, @claude_binary)

      assert proof.status == 0
      assert proof.leaked_token == false
      calls = CustomerAgentClaude.tool_calls(proof.events)

      assert Enum.any?(
               calls,
               &(&1["name"] == "Skill" and &1["input"]["skill"] == "allsource-customer")
             )

      assert Enum.any?(calls, &(&1["name"] == "mcp__allsource_review__allsource_review_context"))

      assert Enum.any?(
               calls,
               &(&1["name"] == "mcp__allsource_review__allsource_validate_review_proposal")
             )

      results = CustomerAgentClaude.results(proof.events)
      assert Enum.any?(results, &(&1["state"] == "eligibility_verified"))

      assert Enum.any?(
               results,
               &(&1["state"] == "valid_unresolved" and &1["approved"] == false and
                   &1["persisted"] == false)
             )
    end)
  end

  defp provision do
    client = RustCoreClient.write_client()

    assert status(
             Tesla.post(client, "/api/v1/tenants", %{
               id: @binding["tenant_id"],
               name: "Synthetic HTTP review"
             })
           ) == 201

    metadata = %{
      "subscription" => %{"tier" => "indie", "status" => "active"},
      "quotas" => %{"mcp_scope" => "read", "queries_quota" => 50_000, "queries_used" => 0}
    }

    assert status(
             Tesla.put(client, "/api/v1/tenants/#{@binding["tenant_id"]}", %{metadata: metadata})
           ) == 200

    assert status(
             Tesla.post(client, "/api/v1/config", %{
               key: "team:#{@binding["tenant_id"]}:members",
               value: [%{user_id: @binding["subject_id"], role: "member"}],
               changed_by: "synthetic-test"
             })
           ) == 200

    assert {:ok, issued} =
             issue(
               @binding,
               ["read_context", "validate_proposal"],
               System.system_time(:second),
               180
             )

    issued
  end

  defp issue(binding, operations, now, ttl) do
    CustomerAgentGrantStore.issue(
      binding,
      operations,
      %{"accepted" => true, "version" => "review-metadata-v1"},
      now,
      ttl
    )
  end

  defp request(context, operation, token, body) do
    client =
      Tesla.client(
        [
          {Tesla.Middleware.BaseUrl, context.query_url},
          {Tesla.Middleware.Headers,
           [
             {"authorization", "Bearer " <> token},
             {"content-type", "application/json"},
             {"accept", "application/json"}
           ]},
          {Tesla.Middleware.Timeout, timeout: 10_000}
        ],
        Tesla.Adapter.Hackney
      )

    case Tesla.post(client, "/api/customer-agent/" <> operation, Jason.encode!(body)) do
      {:ok, response} ->
        decoded =
          case Jason.decode(response.body) do
            {:ok, value} -> value
            _ -> nil
          end

        {response.status, decoded, response.headers}

      {:error, reason} when is_atom(reason) ->
        flunk("Synthetic HTTP request failed: #{reason}")

      _ ->
        flunk("Synthetic HTTP request failed with non-atom transport error")
    end
  end

  defp status({:ok, %{status: code}}), do: code
  defp status(_), do: :request_failed

  defp with_mcp(context, token, fun) do
    env =
      %{
        "ALLSOURCE_CUSTOMER_REVIEW" => "true",
        "CUSTOMER_REVIEW_URL" => context.query_url,
        "CUSTOMER_REVIEW_GRANT" => token,
        "CUSTOMER_REVIEW_TENANT" => @binding["tenant_id"],
        "CUSTOMER_REVIEW_SUBJECT" => @binding["subject_id"],
        "CUSTOMER_REVIEW_CLIENT" => @binding["client_id"],
        "CUSTOMER_REVIEW_RESOURCE" => @binding["resource"],
        "CORE_API_KEY" => "",
        "ALLSOURCE_CORE_API_KEY" => "",
        "CORE_MODE" => "remote",
        "CORE_WS_ENABLED" => "false",
        "ALLSOURCE_SYSTEM_ADMIN" => "true"
      }
      |> Enum.map(fn {key, value} -> {String.to_charlist(key), String.to_charlist(value)} end)

    # test-hang-allow: owned local release process; bounded reads, always killed and awaited.
    port =
      Port.open({:spawn_executable, String.to_charlist(@mcp_binary)}, [
        :binary,
        :exit_status,
        {:line, 65_536},
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

  defp rpc(port, id, method, params) do
    Port.command(
      port,
      Jason.encode!(%{jsonrpc: "2.0", id: id, method: method, params: params}) <> "\n"
    )

    receive do
      {^port, {:data, {:eol, line}}} -> Jason.decode!(line)
      {^port, {:exit_status, _}} -> flunk("Owned MCP process exited unexpectedly")
    after
      15_000 -> flunk("MCP response deadline exceeded")
    end
  end
end
