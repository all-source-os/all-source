defmodule QueryServiceEx.Integration.EventHistoryTest do
  use ExUnit.Case, async: false
  alias QueryServiceEx.Infrastructure.Adapters.CustomerQueryOperationStore
  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient, as: Core
  alias QueryServiceEx.TestSupport.CustomerAgentCore
  alias QueryServiceExWeb.Endpoint
  import Plug.Conn, only: [put_req_header: 3, get_resp_header: 2]
  @moduletag :integration
  @moduletag timeout: 90_000
  @moduletag skip: is_nil(System.get_env("ALLSOURCE_CORE_BINARY"))
  @subject "oauth:github:history-reviewer"
  @origin "https://history.example.test"
  @scope "allsource.history.read"
  @key "synthetic history product session key for integration tests only"

  setup do
    old =
      Map.new(
        [:event_history_enabled, :event_history_origin, :event_history_clients],
        &{&1, Application.get_env(:query_service_ex, &1)}
      )

    env = Map.new(~w(JWT_SECRET ALLSOURCE_HISTORY_GATEWAY_SUBJECT), &{&1, System.get_env(&1)})
    Application.put_env(:query_service_ex, :event_history_enabled, true)
    Application.put_env(:query_service_ex, :event_history_origin, @origin)

    Application.put_env(:query_service_ex, :event_history_clients, %{
      "test-gpt" => %{
        "host" => "ChatGPT",
        "redirect_uri" => "https://chat.example.test/callback",
        "token_endpoint_auth_method" => "none"
      },
      "test-claude" => %{
        "host" => "Claude",
        "redirect_uri" => "https://claude.example.test/callback",
        "token_endpoint_auth_method" => "client_secret_basic",
        "client_secret" => String.duplicate("s", 40)
      }
    })

    System.put_env("JWT_SECRET", @key)
    System.put_env("ALLSOURCE_HISTORY_GATEWAY_SUBJECT", "grant-test-user")

    on_exit(fn ->
      for {k, v} <- old,
          do:
            if(is_nil(v),
              do: Application.delete_env(:query_service_ex, k),
              else: Application.put_env(:query_service_ex, k, v)
            )

      for {k, v} <- env, do: if(is_nil(v), do: System.delete_env(k), else: System.put_env(k, v))
    end)

    CustomerAgentCore.setup_context()
  end

  test "registered hosts select workspaces, read checkout, open exact citations and disconnect",
       context do
    CustomerAgentCore.with_core(context, fn ->
      human = human_token()
      provision("history-a", [@subject])
      provision("history-b", [@subject])
      provision("history-foreign", ["other"])

      released =
        seed("history-a", "deployment.released.v1", %{
          "version" => "2.4",
          "ignored" => "payload-canary"
        })

      rollback =
        seed("history-a", "deployment.rolled_back.v1", %{
          "from_version" => "2.4",
          "to_version" => "2.3"
        })

      foreign = seed("history-foreign", "deployment.released.v1", %{"version" => "private"})
      assert {200, %{"workspaces" => spaces}} = api("workspaces", %{}, human)
      assert Enum.map(spaces, & &1["id"]) == ["history-a", "history-b"]

      for {client, workspace} <- [{"test-gpt", "history-a"}, {"test-claude", "history-b"}] do
        {access, session} = connect(client, workspace, human)
        args = %{session_id: session, request_id: 10, arguments: %{entity: "checkout"}}
        assert {200, %{"data" => history}} = api("timeline", args, access)
        assert history["workspace"]["id"] == workspace

        if workspace == "history-a" do
          assert [first, second] = history["events"]

          assert first["kind"] == "rolled_back" and first["id"] == rollback and
                   first["from_version"] == "2.4" and first["to_version"] == "2.3"

          assert second["kind"] == "released" and second["id"] == released and
                   second["version"] == "2.4"

          refute Jason.encode!(history) =~ "payload-canary"

          for event <- [first, second] do
            id = event["id"]
            assert event["url"] == @origin <> "/event-history/workspaces/history-a/events/" <> id
            data = %{"workspace_id" => workspace, "event_id" => id}
            assert {200, %{"intent" => intent}} = api("evidence-intent", data, human)

            assert {200, %{"event" => ^event}} =
                     api("evidence", Map.put(data, "intent", intent), human)

            assert {409, _} = api("evidence", Map.put(data, "intent", intent), human)
          end

          data = %{"workspace_id" => workspace, "event_id" => foreign}
          assert {200, %{"intent" => intent}} = api("evidence-intent", data, human)

          assert {503, %{"error" => "temporarily_unavailable"}} =
                   api("evidence", Map.put(data, "intent", intent), human)
        else
          assert [] == history["events"]
          assert history["coverage"] in ["partial", "complete"]
        end

        assert {409, _} = api("timeline", args, access)
        assert {409, _} = api("timeline", %{args | arguments: %{entity: "other"}}, access)
        assert {200, %{"connections" => [grant]}} = api("connections", %{}, human)

        assert {200, %{"revoked" => true}} =
                 api("revoke", %{workspace_id: workspace, id: grant["id"]}, human)

        assert {401, _} = api("timeline", %{args | request_id: 11}, access)
      end
    end)
  end

  test "removed member cannot grant, retain access or disclose previously issued evidence intent",
       context do
    CustomerAgentCore.with_core(context, fn ->
      human = human_token()
      provision("history-a", [@subject])
      id = seed("history-a", "deployment.released.v1", %{"version" => "2.4"})
      {access, session} = connect("test-gpt", "history-a", human)
      {request, _} = prepare("test-gpt")
      data = %{"workspace_id" => "history-a", "event_id" => id}
      assert {200, %{"intent" => intent}} = api("evidence-intent", data, human)
      members("history-a", [])

      assert {403, _} =
               api(
                 "oauth/decision",
                 %{decision: "allow", workspace_id: "history-a", request_token: request},
                 human
               )

      assert {401, _} =
               api(
                 "timeline",
                 %{session_id: session, request_id: 1, arguments: %{entity: "checkout"}},
                 access
               )

      assert {404, _} = api("evidence", Map.put(data, "intent", intent), human)
    end)
  end

  test "racing host deliveries spend final unit once and restart preserves operation identity",
       context do
    {access, args} =
      CustomerAgentCore.with_core(context, fn ->
        human = human_token()
        provision("history-a", [@subject], 1)
        seed("history-a", "deployment.released.v1", %{"version" => "2.4"})
        {access, session} = connect("test-gpt", "history-a", human)
        args = %{session_id: session, request_id: "race", arguments: %{entity: "checkout"}}
        deliveries = for _ <- 1..2, do: Task.async(fn -> api("timeline", args, access) end)

        assert Enum.sort(for {status, _} <- Task.await_many(deliveries, 20_000), do: status) == [
                 200,
                 409
               ]

        assert {402, _} = api("timeline", %{args | request_id: "next"}, access)
        assert used("history-a") == 1
        {access, args}
      end)

    CustomerAgentCore.with_core(context, fn ->
      assert {409, _} = api("timeline", args, access)
      assert used("history-a") == 1
    end)
  end

  defmodule UnavailableSource do
    alias QueryServiceEx.Infrastructure.Adapters.EventHistoryStore
    defdelegate grants(subject), to: EventHistoryStore
    def history(_, _), do: {:error, :source_unavailable}
  end

  test "source failure records unsuccessful outcome and prevents second read and charge",
       context do
    CustomerAgentCore.with_core(context, fn ->
      human = human_token()
      provision("history-a", [@subject])
      id = seed("history-a", "deployment.released.v1", %{"version" => "2.4"})
      {access, session} = connect("test-gpt", "history-a", human)
      nav = %{"workspace_id" => "history-a", "event_id" => id}
      assert {200, %{"intent" => intent}} = api("evidence-intent", nav, human)
      evidence = Map.put(nav, "intent", intent)
      lookup = %{session_id: session, request_id: 20, arguments: %{entity: "checkout"}}
      old = Application.get_env(:query_service_ex, :event_history_store)

      try do
        Application.put_env(:query_service_ex, :event_history_store, UnavailableSource)
        assert {503, _} = api("timeline", lookup, access)
        assert {503, _} = api("evidence", evidence, human)
      after
        if old,
          do: Application.put_env(:query_service_ex, :event_history_store, old),
          else: Application.delete_env(:query_service_ex, :event_history_store)
      end

      assert {409, _} = api("timeline", lookup, access)
      assert {409, _} = api("evidence", evidence, human)
      assert used("history-a") == 2

      {:ok, journal, _} = CustomerQueryOperationStore.load("history-a")

      assert Enum.map(Map.values(journal["operations"]), & &1["outcome"]) == ["failed", "failed"]
    end)
  end

  defp used(tenant) do
    assert {:ok, record} = Core.get_tenant_for_authorization(tenant)
    record["metadata"]["quotas"]["queries_used"]
  end

  defp prepare(client) do
    verifier = Base.url_encode64(:crypto.strong_rand_bytes(32), padding: false)

    redirect =
      Application.fetch_env!(:query_service_ex, :event_history_clients)[client]["redirect_uri"]

    params = %{
      client_id: client,
      redirect_uri: redirect,
      response_type: "code",
      scope: @scope,
      resource: @origin <> "/mcp/event-history",
      state: "synthetic-nonce",
      code_challenge_method: "S256",
      ui_locales: "en-US",
      code_challenge: Base.url_encode64(:crypto.hash(:sha256, verifier), padding: false)
    }

    assert {200, %{"request_token" => sealed}} = api("oauth/prepare", params, nil)

    {sealed,
     %{
       client_id: client,
       redirect_uri: redirect,
       grant_type: "authorization_code",
       resource: params.resource,
       code_verifier: verifier
     }}
  end

  defp connect(client, workspace, human) do
    {sealed, token} = prepare(client)

    assert {200, %{"code" => code, "state" => "synthetic-nonce"}} =
             api(
               "oauth/decision",
               %{request_token: sealed, decision: "allow", workspace_id: workspace},
               human
             )

    auth =
      if client == "test-claude",
        do: "Basic " <> Base.encode64(client <> ":" <> String.duplicate("s", 40))

    form = Map.put(token, :code, code)

    assert {400, %{"error" => "invalid_grant"}} =
             api("oauth/token", %{form | code_verifier: String.duplicate("x", 43)}, auth)

    assert {200, %{"access_token" => bearer, "scope" => @scope}} = api("oauth/token", form, auth)
    assert {400, %{"error" => "invalid_grant"}} = api("oauth/token", form, auth)
    access = "Bearer " <> bearer
    assert {200, %{"data" => %{"session_id" => session}}} = api("session", %{}, access)
    {access, session}
  end

  defp api(path, params, auth) do
    form = path == "oauth/token"
    body = if form, do: URI.encode_query(params), else: Jason.encode!(params)

    conn =
      Plug.Test.conn(:post, "/api/event-history/" <> path, body)
      |> put_req_header(
        "content-type",
        if(form, do: "application/x-www-form-urlencoded", else: "application/json")
      )

    conn = if auth, do: put_req_header(conn, "authorization", auth), else: conn
    conn = Endpoint.call(conn, Endpoint.init([]))
    assert [cache] = get_resp_header(conn, "cache-control")
    assert cache =~ "no-store"
    {conn.status, Jason.decode!(conn.resp_body)}
  end

  defp human_token do
    now = System.system_time(:second)

    {_, token} =
      JOSE.JWT.sign(JOSE.JWK.from_oct(@key), %{"alg" => "HS256"}, %{
        "sub" => @subject,
        "tenant_id" => "history-a",
        "email_verified" => true,
        "provider" => "github",
        "iat" => now,
        "exp" => now + 900
      })
      |> JOSE.JWS.compact()

    "Bearer " <> token
  end

  defp provision(id, subjects, quota \\ 100) do
    assert status(Tesla.post(Core.write_client(), "/api/v1/tenants", %{id: id, name: id})) == 201

    assert status(
             Tesla.put(Core.write_client(), "/api/v1/tenants/" <> id, %{
               metadata: %{
                 subscription: %{tier: "indie", status: "active"},
                 quotas: %{mcp_scope: "read", queries_quota: quota, queries_used: 0}
               }
             })
           ) == 200

    members(id, subjects)
  end

  defp members(id, subjects) do
    assert status(
             Tesla.post(Core.write_client(), "/api/v1/config", %{
               key: "team:" <> id <> ":members",
               value: %{
                 schema_version: 2,
                 members: Enum.map(subjects, &%{user_id: &1, role: "member"}),
                 invitations: %{}
               },
               changed_by: "history-test"
             })
           ) == 200
  end

  defp status({:ok, %{status: status}}), do: status
  defp status(_), do: :unavailable

  defp seed(tenant, kind, payload) do
    assert {:ok, %{"event_id" => id}} =
             Core.create_event(tenant, %{
               entity_id: "checkout",
               event_type: kind,
               payload: payload
             })

    id
  end
end
