defmodule QueryServiceEx.Integration.CustomerEvidenceTransportTest do
  use ExUnit.Case, async: false
  import ExUnit.CaptureLog
  import QueryServiceEx.TestSupport.CustomerAgentCore, only: [with_core: 2]
  alias QueryServiceEx.Application.Services.CustomerRemoteAuthorization
  alias QueryServiceEx.Application.Services.CustomerReviewRecords
  alias QueryServiceEx.Infrastructure.Adapters.CustomerAgentGrantStore, as: Grants
  alias QueryServiceEx.Infrastructure.Adapters.CustomerQueryUsageStore, as: Usage
  alias QueryServiceEx.TestSupport.CustomerAgentConnection
  alias QueryServiceEx.TestSupport.CustomerRemoteHTTP, as: HTTP
  alias QueryServiceEx.TestSupport.CustomerRemoteMCP, as: Remote
  alias QueryServiceEx.TestSupport.MeteredEvidenceFixture, as: F

  @moduletag :integration
  @moduletag timeout: 120_000
  @moduletag skip: is_nil(System.get_env("ALLSOURCE_CORE_BINARY"))
  @mcp System.get_env("ALLSOURCE_CUSTOMER_MCP_BINARY")
  @secret "synthetic-only-evidence-transport-secret-2026"

  setup do
    context = F.setup_context()

    config = [
      customer_review_enabled: true,
      customer_evidence_enabled: true,
      customer_connections_enabled: true,
      customer_remote_enabled: true
    ]

    old = for {key, _} <- config, do: {key, Application.get_env(:query_service_ex, key)}
    for {key, value} <- config, do: Application.put_env(:query_service_ex, key, value)
    secret = System.get_env("JWT_SECRET")
    System.put_env("JWT_SECRET", @secret)
    clear_rates()

    on_exit(fn ->
      for {key, value} <- old do
        if is_nil(value),
          do: Application.delete_env(:query_service_ex, key),
          else: Application.put_env(:query_service_ex, key, value)
      end

      if secret, do: System.put_env("JWT_SECRET", secret), else: System.delete_env("JWT_SECRET")
      clear_rates()
    end)

    # test-hang-allow: supervised loopback endpoint, bounded request and shutdown deadlines.
    server =
      start_supervised!({Bandit, plug: QueryServiceExWeb.Endpoint, port: 0, ip: {127, 0, 0, 1}})

    {:ok, {_, port}} = ThousandIsland.listener_info(server)
    Map.merge(context, %{query_url: "http://127.0.0.1:#{port}", evidence: true})
  end

  test "product-only sharing and exact paid retries survive final quota unit and restart",
       context do
    {grant, input, receipt, request, view} =
      with_core(context, fn ->
        now = System.system_time(:second)
        grant = F.provision(4, now)

        refs =
          for number <- [1, 2] do
            source = F.source_input(number, now)
            body = %{"connection_id" => grant.id, "source" => source}

            assert {403, _, _} =
                     HTTP.http(
                       context,
                       :post,
                       "connections/share",
                       body,
                       HTTP.bearer(grant.token)
                     )

            assert {200, %{"data" => shared}, _} =
                     HTTP.http(
                       context,
                       :post,
                       "connections/share",
                       body,
                       HTTP.bearer(session(now))
                     )

            assert {200, %{"data" => ^shared}, _} =
                     HTTP.http(
                       context,
                       :post,
                       "connections/share",
                       body,
                       HTTP.bearer(session(now))
                     )

            shared["source"]
          end

        input = %{
          "expected_revision" => 0,
          "idempotency_key" => F.operation(999, now),
          "proposal" => %{
            "schema_version" => 1,
            "kind" => "run_comparison",
            "projection_name" => nil,
            "sources" => refs
          }
        }

        assert {200, %{"data" => %{"preparation_available" => true}}, _} =
                 request(context, "context", grant, %{})

        assert {200, %{"data" => receipt}, _} = request(context, "prepare", grant, input)
        assert receipt["state"] == "pending"
        assert receipt["approved"] == false
        assert used() == 4
        assert {200, %{"data" => ^receipt}, _} = request(context, "prepare", grant, input)
        assert used() == 4

        assert {409, _, _} =
                 request(
                   context,
                   "prepare",
                   grant,
                   put_in(input, ["proposal", "sources"], Enum.reverse(refs))
                 )

        request = read_request(receipt, now)
        assert {402, _, _} = request(context, "review", grant, request)
        F.set_quota(6)
        assert {200, %{"data" => view}, _} = request(context, "review", grant, request)
        assert view["evidence"]["state"] == "divergent"
        assert used() == 6
        {grant, input, receipt, request, view}
      end)

    with_core(context, fn ->
      assert {200, %{"data" => ^receipt}, _} = request(context, "prepare", grant, input)
      assert {200, %{"data" => ^view}, _} = request(context, "review", grant, request)
      assert used() == 6
      source = hd(input["proposal"]["sources"])["ref"]
      assert :ok = CustomerReviewRecords.revoke(F.tenant(), "sources", source)
      assert {200, %{"data" => hidden}, _} = request(context, "review", grant, request)
      assert hidden["state"] == "unavailable"
      refute Map.has_key?(hidden, "evidence")
      assert used() == 6
    end)
  end

  test "disabled flags, metadata consent, forged authority and source input stay closed and private",
       context do
    with_core(context, fn ->
      now = System.system_time(:second)
      grant = F.provision(20, now)
      input = F.prepare_input(grant, now)
      marker = "PRIVATE-EVIDENCE-TRANSPORT-FIXTURE"

      log =
        capture_log(fn ->
          assert {400, _, _} =
                   request(context, "prepare", grant, Map.put(input, "approved", marker))

          assert {403, _, _} = request(context, "prepare?secret=" <> marker, grant, input)

          assert {413, _, _} =
                   request(
                     context,
                     "prepare",
                     grant,
                     Map.put(input, "proposal", String.duplicate(marker, 4000))
                   )

          assert {403, _, _} =
                   HTTP.http(
                     context,
                     :post,
                     "prepare",
                     Map.put(input, "binding", Map.put(F.binding(), "tenant_id", "foreign")),
                     HTTP.bearer(grant.token)
                   )
        end)

      refute log =~ marker
      refute log =~ grant.token
      refute log =~ F.binding()["subject_id"]
      Application.put_env(:query_service_ex, :customer_evidence_enabled, false)
      assert {403, _, _} = request(context, "prepare", grant, input)

      assert {200, %{"data" => %{"preparation_available" => false}}, _} =
               request(context, "context", grant, %{})

      Application.put_env(:query_service_ex, :customer_evidence_enabled, true)

      assert {:ok, metadata} =
               Grants.issue(
                 F.binding(),
                 ~w(read_context validate_proposal),
                 %{"accepted" => true, "version" => "review-metadata-v1"},
                 now,
                 600
               )

      assert {403, _, _} = request(context, "prepare", metadata, input)
      assert used() == 2
      F.set_members([])
      assert {403, _, _} = request(context, "prepare", grant, input)
      assert used() == 2
    end)
  end

  @tag skip: is_nil(@mcp)
  test "compiled stdio discovers evidence tools and returns schema-valid matching pending views",
       context do
    with_core(context, fn ->
      now = System.system_time(:second)
      grant = F.provision(8, now)
      input = F.prepare_input(grant, now)

      with_stdio(context, grant, fn port ->
        assert %{"result" => %{"tools" => tools}} = rpc(port, 1, "tools/list", %{})

        assert Enum.map(tools, & &1["name"]) ==
                 ~w(allsource_review_context allsource_validate_review_proposal allsource_prepare_review allsource_get_review allsource_get_review_result)

        receipt = tool(port, 2, "allsource_prepare_review", input)
        assert receipt["state"] == "pending"
        assert tool(port, 3, "allsource_prepare_review", input) == receipt
        args = read_request(receipt, now)
        view = tool(port, 4, "allsource_get_review", args)
        assert view["evidence"]["state"] == "divergent"
        result = tool(port, 5, "allsource_get_review_result", args)
        assert result["result_available"] == false
        assert result["approved"] == false
        assert used() == 8
        assert tool(port, 6, "allsource_get_review_result", args) == result

        assert %{"error" => %{"code" => -32_602}} =
                 rpc(port, 7, "tools/call", %{"name" => "approve_review", "arguments" => args})

        assert :ok = Grants.revoke(F.binding(), grant.id, now)

        assert %{"result" => %{"isError" => true}} =
                 rpc(port, 8, "tools/call", %{
                   "name" => "allsource_get_review",
                   "arguments" => args
                 })
      end)
    end)
  end

  @tag skip: is_nil(@mcp)
  test "compiled remote HTTP preflight permits paid retries at zero quota and denies revoked grant",
       context do
    with_core(context, fn ->
      now = System.system_time(:second)
      binding = Map.put(F.binding(), "client_id", "claude-ai")
      grant = F.provision(4, now, binding)
      assert :ok = Grants.activate_remote(grant.token, binding, now)
      input = F.prepare_input(grant, now)

      assert {:ok, envelope} =
               CustomerRemoteAuthorization.seal_access(
                 %{token: grant.token, binding: binding, expires_at: now + 600},
                 now
               )

      Remote.with_mcp(context, fn url ->
        message = %{
          jsonrpc: "2.0",
          id: 1,
          method: "tools/call",
          params: %{name: "allsource_prepare_review", arguments: input}
        }

        assert {200, %{"result" => %{"isError" => false, "structuredContent" => receipt}}, _} =
                 Remote.http(url, :post, envelope, message)

        assert used() == 4

        assert {200, %{"result" => %{"isError" => false, "structuredContent" => ^receipt}}, _} =
                 Remote.http(url, :post, envelope, message)

        assert used() == 4
        assert :ok = Grants.revoke(binding, grant.id, now)
        assert {401, _, _} = Remote.http(url, :post, envelope, message)
      end)
    end)
  end

  defp request(context, operation, grant, input),
    do:
      HTTP.http(
        context,
        :post,
        operation,
        Map.put(input, "binding", F.binding()),
        HTTP.bearer(grant.token)
      )

  defp read_request(receipt, now),
    do: %{"id" => receipt["id"], "version" => 1, "request_id" => F.operation(1000, now)}

  defp used do
    assert {:ok, %{"used" => used}} = Usage.snapshot(F.tenant())
    used
  end

  defp session(now) do
    claims =
      Map.merge(F.actor(), %{
        "sub" => F.actor()["subject_id"],
        "iat" => now,
        "exp" => now + 600,
        "provider" => "google",
        "email_verified" => true
      })

    {_, token} =
      JOSE.JWT.sign(JOSE.JWK.from_oct(@secret), %{"alg" => "HS256"}, claims) |> JOSE.JWS.compact()

    token
  end

  defp clear_rates do
    for suffix <- [
          "customer-review:admission",
          "customer-remote:admission",
          "customer-connections:admission",
          "customer-review:" <> F.tenant(),
          "customer-connections:" <> F.tenant()
        ],
        do: :ets.delete(:rate_limiter_buckets, suffix)
  end

  defp with_stdio(context, grant, fun) do
    path = CustomerAgentConnection.write(context, grant.token, F.binding())

    env = %{
      "ALLSOURCE_CUSTOMER_REVIEW" => "true",
      "ALLSOURCE_CUSTOMER_EVIDENCE_REVIEW" => "true",
      "ALLSOURCE_CUSTOMER_REVIEW_HTTP" => "false",
      "CUSTOMER_REVIEW_CONNECTION_FILE" => path,
      "CORE_API_KEY" => "",
      "ALLSOURCE_CORE_API_KEY" => "",
      "CORE_WS_ENABLED" => "false"
    }

    env =
      Enum.map(env, fn {key, value} -> {String.to_charlist(key), String.to_charlist(value)} end)

    # test-hang-allow: owned release, bounded reads and unconditional cleanup.
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
            5000 -> flunk("Owned MCP process did not exit")
          end

        nil ->
          :ok
      end
    end
  end

  defp tool(port, id, name, arguments) do
    response = rpc(port, id, "tools/call", %{"name" => name, "arguments" => arguments})

    assert %{
             "result" => %{
               "isError" => false,
               "structuredContent" => data,
               "content" => [%{"text" => text}]
             }
           } = response

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
