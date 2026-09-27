defmodule QueryServiceEx.Integration.CustomerRemoteAuthorizationTest do
  use ExUnit.Case, async: false
  alias QueryServiceEx.Application.Services.CustomerAgentAccess
  alias QueryServiceEx.Application.Services.CustomerConnections
  alias QueryServiceEx.Application.Services.CustomerRemoteAuthorization
  alias QueryServiceEx.Infrastructure.Adapters.CustomerAgentGrantStore
  alias QueryServiceEx.Infrastructure.Adapters.CustomerAuthorizationCode
  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient
  alias QueryServiceEx.TestSupport.CustomerAgentCore
  import QueryServiceEx.TestSupport.CustomerAgentCore, only: [with_core: 2]

  @moduletag :integration
  @moduletag timeout: 90_000
  @moduletag skip: is_nil(System.get_env("ALLSOURCE_CORE_BINARY"))
  @tenant "grant-test-tenant"
  @subject "oauth:google:remote-synthetic"
  @resource "https://api.example.test/mcp"
  @actor %{"tenant_id" => @tenant, "subject_id" => @subject}
  @consent %{"accepted" => true, "version" => "review-metadata-v1"}
  @verifier "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
  @request %{
    "client_id" => "claude-ai",
    "redirect_uri" => "https://claude.ai/api/mcp/auth_callback",
    "response_type" => "code",
    "resource" => @resource,
    "scope" => "allsource.review",
    "state" => "synthetic-oauth-state",
    "code_challenge_method" => "S256",
    "code_challenge" => "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
  }

  setup do
    context = CustomerAgentCore.setup_context()
    previous_secret = System.get_env("JWT_SECRET")
    previous_resource = Application.get_env(:query_service_ex, :customer_review_resource)
    System.put_env("JWT_SECRET", "synthetic-only-remote-code-secret-2026")
    Application.put_env(:query_service_ex, :customer_review_resource, @resource)

    on_exit(fn ->
      if previous_secret,
        do: System.put_env("JWT_SECRET", previous_secret),
        else: System.delete_env("JWT_SECRET")

      if previous_resource,
        do: Application.put_env(:query_service_ex, :customer_review_resource, previous_resource),
        else: Application.delete_env(:query_service_ex, :customer_review_resource)
    end)

    context
  end

  test "pending code, activation and replay revocation survive SIGKILL; old registry cannot revive",
       context do
    {issued, payload, snapshot} =
      with_core(context, fn ->
        provision()
        {issued, payload} = authorize()
        assert {:error, :access_denied} = access(payload)

        assert {:ok, [%{"status" => "pending"} = receipt]} =
                 CustomerAgentGrantStore.list(@tenant, @subject, now())

        assert receipt["consent"]["accepted_at"] == payload["created_at"]
        assert {:ok, snapshot, _} = RustCoreClient.get_customer_connection_registry(@tenant)
        refute Jason.encode!(snapshot) =~ payload["token"]
        refute issued.code =~ payload["token"]
        refute issued.code =~ @subject
        refute issued.code =~ @tenant
        refute issued.code =~ @verifier
        {issued, payload, snapshot}
      end)

    with_core(context, fn ->
      assert {:error, :access_denied} = access(payload)
      assert {:ok, redeemed} = CustomerRemoteAuthorization.redeem(exchange(issued.code), now())
      assert redeemed.token == payload["token"]
      assert redeemed.binding == payload["binding"]
      assert redeemed.expires_at == payload["created_at"] + 3_600
      assert {:ok, _} = access(payload)
    end)

    with_core(context, fn ->
      assert {:ok, _} = access(payload)

      assert {:error, :invalid_grant} =
               CustomerRemoteAuthorization.redeem(exchange(issued.code), now())

      assert {:error, :access_denied} = access(payload)
      assert {:ok, _, revision} = RustCoreClient.get_customer_connection_registry(@tenant)
      assert :ok = RustCoreClient.put_customer_connection_registry(@tenant, snapshot, revision)
      assert {:error, :access_denied} = access(payload)
    end)

    with_core(context, fn ->
      assert {:error, :access_denied} = access(payload)

      assert {:ok, [%{"status" => "revoked"}]} =
               CustomerAgentGrantStore.list(@tenant, @subject, now())
    end)
  end

  test "substitution, code tampering, session tokens and missing secret never activate",
       context do
    with_core(context, fn ->
      provision()
      {issued, payload} = authorize()

      for {key, value} <- [
            {"client_id", "claude-code"},
            {"redirect_uri", @request["redirect_uri"] <> "?x=1"},
            {"resource", @resource <> "/"},
            {"code_verifier", String.duplicate("x", 43)},
            {"code", CustomerAgentCore.token("admin")},
            {"code", issued.code <> "x"},
            {"tenant_id", "different-tenant"}
          ] do
        assert {:error, :invalid_grant} =
                 CustomerRemoteAuthorization.redeem(
                   Map.put(exchange(issued.code), key, value),
                   now()
                 )

        assert {:error, :access_denied} = access(payload)
      end

      secret = System.fetch_env!("JWT_SECRET")
      System.delete_env("JWT_SECRET")

      assert {:error, :invalid_grant} =
               CustomerRemoteAuthorization.redeem(exchange(issued.code), now())

      System.put_env("JWT_SECRET", secret)
      assert {:ok, _} = CustomerRemoteAuthorization.redeem(exchange(issued.code), now())
      assert {:ok, _} = access(payload)

      assert {:error, :invalid_grant} =
               CustomerRemoteAuthorization.redeem(
                 exchange(issued.code),
                 payload["created_at"] + 300
               )
    end)
  end

  test "membership, entitlement and pending revocation remain live before exchange", context do
    with_core(context, fn ->
      provision()
      {issued, payload} = authorize()
      set_members([])

      assert {:error, :invalid_grant} =
               CustomerRemoteAuthorization.redeem(exchange(issued.code), now())

      set_members([%{"user_id" => @subject, "role" => "member"}])
      set_billing("canceled")

      assert {:error, :invalid_grant} =
               CustomerRemoteAuthorization.redeem(exchange(issued.code), now())

      set_billing("active")
      assert {:ok, %{connections: [receipt]}} = CustomerConnections.list(@actor, now())
      assert {:ok, %{revoked: true}} = CustomerConnections.revoke(@actor, receipt["id"], now())

      assert {:error, :invalid_grant} =
               CustomerRemoteAuthorization.redeem(exchange(issued.code), now())

      assert {:error, :access_denied} = access(payload)
    end)
  end

  test "concurrent exchange returns at most one credential and replay denies every caller",
       context do
    with_core(context, fn ->
      provision()
      {issued, payload} = authorize()

      results =
        1..8
        |> Task.async_stream(
          fn _ -> CustomerRemoteAuthorization.redeem(exchange(issued.code), now()) end,
          timeout: 25_000,
          max_concurrency: 8
        )
        |> Enum.map(fn {:ok, value} -> value end)

      assert Enum.count(results, &match?({:ok, _}, &1)) <= 1

      assert Enum.all?(
               results,
               &(match?({:ok, _}, &1) or
                   &1 in [{:error, :invalid_grant}, {:error, :access_denied}])
             )

      assert {:error, :access_denied} = access(payload)
    end)
  end

  test "bad consent issues nothing; pending requests consume existing connection limit",
       context do
    with_core(context, fn ->
      provision()

      assert {:error, :invalid_consent} =
               CustomerRemoteAuthorization.authorize(@actor, @request, %{}, now())

      assert {:error, :not_found} = RustCoreClient.get_customer_connection_registry(@tenant)
      for _ <- 1..16, do: authorize()

      assert {:error, :connection_limit} =
               CustomerRemoteAuthorization.authorize(@actor, @request, @consent, now())

      assert {:ok, records} = CustomerAgentGrantStore.list(@tenant, @subject, now())
      assert length(records) == 16
      assert Enum.all?(records, &(&1["status"] == "pending"))
    end)
  end

  test "activation receipt is admin-only, cannot use unconditional helper and malformed state denies",
       context do
    with_core(context, fn ->
      provision()
      {issued, payload} = authorize()
      assert {:ok, %{connections: [receipt]}} = CustomerConnections.list(@actor, now())
      key = "customer_agent_v2.remote_redeemed." <> receipt["id"]
      assert {:error, :invalid_key} = RustCoreClient.put_config_for_authorization(key, %{})
      assert {:ok, _} = CustomerRemoteAuthorization.redeem(exchange(issued.code), now())

      client =
        Tesla.client(
          [
            {Tesla.Middleware.BaseUrl, context.url},
            {Tesla.Middleware.Headers,
             [{"authorization", "Bearer " <> CustomerAgentCore.token("developer")}]},
            {Tesla.Middleware.Timeout, timeout: 1_000}
          ],
          Tesla.Adapter.Hackney
        )

      assert {:ok, %{status: 403}} = Tesla.get(client, "/api/v1/config/" <> key)

      assert {:ok, %{status: 200}} =
               Tesla.post(RustCoreClient.write_client(), "/api/v1/config", %{
                 key: key,
                 value: %{"version" => 1, "token_hash" => "bad", "redeemed_at" => now()},
                 changed_by: "synthetic-malformed-receipt"
               })

      assert {:error, :storage_unavailable} = access(payload)

      assert {:error, :invalid_grant} =
               CustomerRemoteAuthorization.redeem(exchange(issued.code), now())

      assert {:error, :access_denied} = access(payload)
    end)
  end

  defp authorize do
    assert {:ok, issued} =
             CustomerRemoteAuthorization.authorize(@actor, @request, @consent, now())

    assert {:ok, payload} = CustomerAuthorizationCode.open(issued.code)
    {issued, payload}
  end

  defp exchange(code),
    do: %{
      "client_id" => "claude-ai",
      "redirect_uri" => @request["redirect_uri"],
      "resource" => @resource,
      "grant_type" => "authorization_code",
      "code_verifier" => @verifier,
      "code" => code
    }

  defp access(payload),
    do: CustomerAgentAccess.verify(payload["token"], payload["binding"], "read_context", now())

  defp now, do: System.system_time(:second)

  defp provision do
    assert {:ok, %{status: 201}} =
             Tesla.post(RustCoreClient.write_client(), "/api/v1/tenants", %{
               id: @tenant,
               name: "Synthetic remote authorization"
             })

    set_members([%{"user_id" => @subject, "role" => "member"}])
    set_billing("active")
  end

  defp set_members(members) do
    assert {:ok, %{status: 200}} =
             Tesla.post(RustCoreClient.write_client(), "/api/v1/config", %{
               key: "team:#{@tenant}:members",
               value: %{"schema_version" => 2, "members" => members, "invitations" => %{}},
               changed_by: "synthetic-test"
             })
  end

  defp set_billing(status) do
    assert {:ok, %{status: 200}} =
             Tesla.put(RustCoreClient.write_client(), "/api/v1/tenants/#{@tenant}", %{
               metadata: %{
                 "subscription" => %{"tier" => "indie", "status" => status},
                 "quotas" => %{"mcp_scope" => "read", "queries_quota" => 100, "queries_used" => 0}
               }
             })
  end
end
