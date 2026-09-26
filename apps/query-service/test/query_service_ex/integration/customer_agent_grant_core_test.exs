defmodule QueryServiceEx.Integration.CustomerAgentGrantCoreTest do
  @moduledoc """
  Real Core admin boundary and WAL recovery for customer connection credentials.

  Run with ALLSOURCE_CORE_BINARY pointing at Core built with `--features enterprise` and
  `mix test --include integration <this file>`. Every Core process uses a private
  temporary directory, synthetic signing secret, loopback listener and no dev
  auth bypass. Processes are killed between phases, not cleanly checkpointed.
  """
  use ExUnit.Case, async: false

  alias QueryServiceEx.Application.Services.CustomerAgentAccess
  alias QueryServiceEx.Infrastructure.Adapters.CustomerAgentGrantStore
  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient
  alias QueryServiceEx.TestSupport.CustomerAgentCore

  @moduletag :integration
  @moduletag timeout: 60_000
  @binary System.get_env("ALLSOURCE_CORE_BINARY")
  @moduletag skip: is_nil(@binary)
  import QueryServiceEx.TestSupport.CustomerAgentCore, only: [with_core: 2, token: 1]

  setup do
    CustomerAgentCore.setup_context()
  end

  test "grants and independent revocations survive Core crashes; tenant writes cannot restore access",
       context do
    binding = %{
      "tenant_id" => "grant-test-tenant",
      "subject_id" => "grant-test-subject",
      "client_id" => "claude-code",
      "resource" => "https://api.example.test/customer-review"
    }

    now = System.system_time(:second)

    issued =
      with_core(context, fn ->
        client = RustCoreClient.write_client()

        response =
          Tesla.post(client, "/api/v1/tenants", %{
            id: binding["tenant_id"],
            name: "Synthetic grant test"
          })

        assert response_status(response) == 201

        assert {:ok, issued} = CustomerAgentGrantStore.issue(binding, ["read_context"], now, 120)

        assert {:ok, _} =
                 CustomerAgentGrantStore.verify_credential(
                   issued.token,
                   binding,
                   "read_context",
                   now
                 )

        assert_admin_boundary(issued.id)
        issued
      end)

    with_core(context, fn ->
      # A new Core process reconstructs the issued grant from the system WAL.
      assert {:ok, _} =
               CustomerAgentGrantStore.verify_credential(
                 issued.token,
                 binding,
                 "read_context",
                 now + 1
               )

      {:ok, tenant} = RustCoreClient.get_tenant_for_authorization(binding["tenant_id"])

      {:ok, grant} =
        RustCoreClient.get_config_for_authorization("customer_agent_v1.grant." <> issued.id)

      assert :ok = CustomerAgentGrantStore.revoke(binding, issued.id, now + 1)

      # Exercise the actual full-map replacement used by billing, not just PATCH.
      response =
        Tesla.put(
          RustCoreClient.write_client(),
          "/api/v1/tenants/#{binding["tenant_id"]}",
          %{metadata: tenant["metadata"]}
        )

      assert response_status(response) == 200

      assert {:ok, _} =
               RustCoreClient.put_config_for_authorization(
                 "customer_agent_v1.grant." <> issued.id,
                 grant
               )

      assert {:error, :unauthorized} =
               CustomerAgentGrantStore.verify_credential(
                 issued.token,
                 binding,
                 "read_context",
                 now + 2
               )
    end)

    with_core(context, fn ->
      # Another hard restart must retain the independent revocation marker.
      assert {:error, :unauthorized} =
               CustomerAgentGrantStore.verify_credential(
                 issued.token,
                 binding,
                 "read_context",
                 now + 3
               )

      assert :ok = CustomerAgentGrantStore.revoke(binding, issued.id, now + 3)
    end)
  end

  test "live eligibility follows actual tenant billing and Control Plane membership after reconnect",
       context do
    binding = %{
      "tenant_id" => "live-access-tenant",
      "subject_id" => "oauth:google:123456789",
      "client_id" => "claude-code",
      "resource" => "https://api.example.test/customer-review"
    }

    now = System.system_time(:second)

    metadata = %{
      "subscription" => %{"tier" => "indie", "status" => "active"},
      "quotas" => %{"mcp_scope" => "read", "queries_quota" => 50_000, "queries_used" => 20}
    }

    members = [
      %{"user_id" => binding["subject_id"], "role" => "member", "email" => "private@example.test"}
    ]

    issued =
      with_core(context, fn ->
        client = RustCoreClient.write_client()

        assert response_status(
                 Tesla.post(client, "/api/v1/tenants", %{
                   id: binding["tenant_id"],
                   name: "Synthetic live access"
                 })
               ) == 201

        set_metadata(client, binding, metadata)
        set_members(client, binding, members)
        assert {:ok, issued} = CustomerAgentGrantStore.issue(binding, ["read_context"], now, 120)

        assert {:ok, context} =
                 CustomerAgentAccess.verify(issued.token, binding, "read_context", now)

        assert context["membership_role"] == "member"
        assert context["queries_remaining"] == 49_980
        refute Jason.encode!(context) =~ "private@example.test"
        refute Jason.encode!(context) =~ issued.token
        issued
      end)

    with_core(context, fn ->
      client = RustCoreClient.write_client()
      assert {:ok, _} = CustomerAgentAccess.verify(issued.token, binding, "read_context", now)

      # Membership revocation must matter even while the credential is valid.
      set_members(client, binding, [])

      assert {:ok, _} =
               CustomerAgentGrantStore.verify_credential(
                 issued.token,
                 binding,
                 "read_context",
                 now
               )

      assert {:error, :access_denied} =
               CustomerAgentAccess.verify(issued.token, binding, "read_context", now)

      set_members(client, binding, [
        %{"user_id" => binding["subject_id"], "role" => "serviceaccount"}
      ])

      assert {:error, :access_denied} =
               CustomerAgentAccess.verify(issued.token, binding, "read_context", now)

      set_members(client, binding, members)

      # An independent caller sees current storage, not an inherited session.
      result =
        Task.async(fn ->
          CustomerAgentAccess.verify(issued.token, binding, "read_context", now)
        end)

      assert {:ok, _} = Task.await(result, 10_000)

      for change <- [
            put_in(metadata, ["subscription", "status"], "canceled"),
            put_in(metadata, ["quotas", "mcp_scope"], ""),
            put_in(metadata, ["quotas", "queries_used"], 50_000),
            put_in(metadata, ["subscription"], %{
              "status" => "active",
              "tier" => "trial",
              "trial_expires_at" => DateTime.from_unix!(now) |> DateTime.to_iso8601()
            })
          ] do
        set_metadata(client, binding, change)

        assert {:error, :access_denied} =
                 CustomerAgentAccess.verify(issued.token, binding, "read_context", now)
      end

      set_metadata(client, binding, put_in(metadata, ["subscription", "status"], "past_due"))
      assert {:ok, _} = CustomerAgentAccess.verify(issued.token, binding, "read_context", now)

      assert {:error, :access_denied} =
               CustomerAgentAccess.verify(
                 issued.token,
                 Map.put(binding, "subject_id", "oauth:google:other"),
                 "read_context",
                 now
               )

      assert {:error, :access_denied} =
               CustomerAgentAccess.verify(issued.token, binding, "approve", now)

      assert :ok = CustomerAgentGrantStore.revoke(binding, issued.id, now)

      assert {:error, :access_denied} =
               CustomerAgentAccess.verify(issued.token, binding, "read_context", now)
    end)
  end

  defp set_metadata(client, binding, metadata) do
    response = Tesla.put(client, "/api/v1/tenants/#{binding["tenant_id"]}", %{metadata: metadata})
    assert response_status(response) == 200
  end

  defp set_members(client, binding, members) do
    response =
      Tesla.post(client, "/api/v1/config", %{
        key: "team:#{binding["tenant_id"]}:members",
        value: members,
        changed_by: "synthetic-test"
      })

    assert response_status(response) == 200
  end

  defp assert_admin_boundary(id) do
    admin = Application.fetch_env!(:query_service_ex, :core_api_key)
    Application.put_env(:query_service_ex, :core_api_key, "Bearer " <> token("developer"))

    assert {:error, :storage_unavailable} =
             RustCoreClient.get_config_for_authorization("customer_agent_v1.grant." <> id)

    assert {:error, :storage_unavailable} =
             RustCoreClient.put_config_for_authorization("customer_agent_v1.revoked." <> id, %{
               "revoked" => true
             })

    Application.put_env(:query_service_ex, :core_api_key, admin)
  end

  # Avoid printing Tesla.Env, which contains the synthetic Authorization header.
  defp response_status({:ok, %{status: status}}), do: status
  defp response_status(_), do: :request_failed
end
