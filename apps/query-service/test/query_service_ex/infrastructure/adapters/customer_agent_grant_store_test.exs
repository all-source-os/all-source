defmodule QueryServiceEx.Infrastructure.Adapters.CustomerAgentGrantStoreTest do
  use ExUnit.Case, async: false

  alias QueryServiceEx.Infrastructure.Adapters.CustomerAgentGrantStore
  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient

  defmodule CoreFixture do
    @moduledoc false
    import Plug.Conn

    def init(opts), do: opts

    def call(conn, _opts) do
      Agent.update(CoreState, &Map.update!(&1, :requests, fn n -> n + 1 end))

      case {conn.method, conn.path_info} do
        {"GET", ["api", "v1", "config", key]} ->
          read_config(conn, key)

        {"POST", ["api", "v1", "config"]} ->
          {:ok, body, conn} = read_body(conn, length: 65_536, read_timeout: 1_000)
          %{"key" => key, "value" => value} = Jason.decode!(body)

          if Agent.get(CoreState, & &1.fail_write) do
            respond(conn, 403, %{"error" => "synthetic-private-error"})
          else
            Agent.update(CoreState, &put_in(&1, [:configs, key], value))
            respond(conn, 200, %{"key" => key, "saved" => true})
          end

        {"GET", ["api", "v1", "tenants", tenant]} ->
          state = Agent.get(CoreState, & &1)

          if state.fail_read,
            do: respond(conn, 503, %{"error" => "synthetic-private-error"}),
            else: respond(conn, 200, Map.put(state.tenant, "tenant_id", tenant))

        {"PATCH", ["api", "v1", "tenants", _tenant, "metadata"]} ->
          {:ok, body, conn} = read_body(conn, length: 65_536, read_timeout: 1_000)
          partial = Jason.decode!(body)

          if Agent.get(CoreState, & &1.fail_write) do
            respond(conn, 403, %{"error" => "synthetic-private-error"})
          else
            Agent.update(CoreState, fn state ->
              update_in(state, [:tenant, "metadata"], &merge(&1, partial))
            end)

            respond(conn, 200, %{})
          end
      end
    end

    defp read_config(conn, key) do
      state = Agent.get(CoreState, & &1)

      cond do
        state.fail_read or key in state.failed_keys ->
          respond(conn, 503, %{"error" => "synthetic-private-error"})

        Map.has_key?(state.configs, key) ->
          respond(conn, 200, %{"key" => key, "value" => state.configs[key]})

        true ->
          respond(conn, 404, %{"error" => "not found"})
      end
    end

    defp merge(left, right) do
      Map.merge(left, right, fn
        _key, old, new when is_map(old) and is_map(new) -> merge(old, new)
        _key, _old, new -> new
      end)
    end

    defp respond(conn, status, body) do
      conn |> put_resp_content_type("application/json") |> send_resp(status, Jason.encode!(body))
    end
  end

  setup do
    start_supervised!(%{
      id: CoreState,
      start:
        {Agent, :start_link,
         [
           fn ->
             %{
               tenant: %{
                 "tenant_id" => "tenant-1",
                 "metadata" => %{"quotas" => %{"events" => 100}}
               },
               requests: 0,
               fail_write: false,
               fail_read: false,
               failed_keys: [],
               configs: %{}
             }
           end,
           [name: CoreState]
         ]}
    })

    # test-hang-allow: ephemeral local HTTP fixture; bounded body reads and client timeout.
    server = start_supervised!({Bandit, plug: CoreFixture, port: 0, ip: {127, 0, 0, 1}})
    {:ok, {_address, port}} = ThousandIsland.listener_info(server)
    previous = Application.get_env(:query_service_ex, :core_write_url)
    Application.put_env(:query_service_ex, :core_write_url, "http://127.0.0.1:#{port}")

    on_exit(fn ->
      if previous,
        do: Application.put_env(:query_service_ex, :core_write_url, previous),
        else: Application.delete_env(:query_service_ex, :core_write_url)
    end)

    %{
      binding: %{
        "tenant_id" => "tenant-1",
        "subject_id" => "subject-1",
        "client_id" => "claude-code",
        "resource" => "https://api.example.test/customer-review"
      }
    }
  end

  test "writes only a hash to Core and verifies with a fresh leader read", %{binding: binding} do
    assert {:ok, issued} = CustomerAgentGrantStore.issue(binding, ["read_context"], 1_000, 60)
    assert issued.token =~ "asreview_v1_"
    state = Agent.get(CoreState, & &1)
    refute Jason.encode!(state) =~ issued.token
    assert state.tenant["metadata"] == %{"quotas" => %{"events" => 100}}
    assert map_size(state.configs) == 1

    assert {:ok, grant} =
             CustomerAgentGrantStore.verify_credential(
               issued.token,
               binding,
               "read_context",
               1_001
             )

    assert grant["subject_id"] == "subject-1"
    refute Map.has_key?(grant, "token_hash")

    before = Agent.get(CoreState, & &1.requests)

    assert {:ok, _} =
             CustomerAgentGrantStore.verify_credential(
               issued.token,
               binding,
               "read_context",
               1_002
             )

    assert Agent.get(CoreState, & &1.requests) == before + 2
  end

  test "revocation survives a new caller and prevents reconnect", %{binding: binding} do
    {:ok, issued} = CustomerAgentGrantStore.issue(binding, ["read_context"], 1_000, 60)
    assert :ok = CustomerAgentGrantStore.revoke(binding, issued.id, 1_001)
    assert :ok = CustomerAgentGrantStore.revoke(binding, issued.id, 1_002)

    result =
      Task.async(fn ->
        CustomerAgentGrantStore.verify_credential(issued.token, binding, "read_context", 1_003)
      end)

    assert Task.await(result, 6_000) == {:error, :unauthorized}
  end

  test "an older full tenant metadata write cannot restore a revoked credential", %{
    binding: binding
  } do
    {:ok, issued} = CustomerAgentGrantStore.issue(binding, ["read_context"], 1_000, 60)
    previous_metadata = Agent.get(CoreState, & &1.tenant["metadata"])
    assert :ok = CustomerAgentGrantStore.revoke(binding, issued.id, 1_001)

    # Billing persists a complete metadata map read before its own merge. This
    # interleaving must not overwrite a customer credential's later revocation.
    assert {:ok, _} =
             RustCoreClient.merge_tenant_metadata(binding["tenant_id"], previous_metadata)

    assert {:error, :unauthorized} =
             CustomerAgentGrantStore.verify_credential(
               issued.token,
               binding,
               "read_context",
               1_002
             )
  end

  test "a late grant-record write cannot overwrite a separate revocation marker", %{
    binding: binding
  } do
    {:ok, issued} = CustomerAgentGrantStore.issue(binding, ["read_context"], 1_000, 60)
    key = "customer_agent_v1.grant." <> issued.id
    {:ok, previous} = RustCoreClient.get_config_for_authorization(key)
    assert :ok = CustomerAgentGrantStore.revoke(binding, issued.id, 1_001)
    assert {:ok, _} = RustCoreClient.put_config_for_authorization(key, previous)

    assert {:error, :unauthorized} =
             CustomerAgentGrantStore.verify_credential(
               issued.token,
               binding,
               "read_context",
               1_002
             )
  end

  test "binds tenant, owner, client, resource and operations", %{binding: binding} do
    {:ok, issued} = CustomerAgentGrantStore.issue(binding, ["read_context"], 1_000, 60)

    for {field, value} <- [
          {"tenant_id", "tenant-other"},
          {"subject_id", "subject-other"},
          {"client_id", "another-host"},
          {"resource", "https://other.example.test/customer-review"}
        ] do
      wrong = Map.put(binding, field, value)

      assert {:error, :unauthorized} =
               CustomerAgentGrantStore.verify_credential(
                 issued.token,
                 wrong,
                 "read_context",
                 1_001
               )

      assert {:error, :unauthorized} = CustomerAgentGrantStore.revoke(wrong, issued.id, 1_001)
    end

    for operation <- ["approve", "execute", "prepare_proposal"] do
      assert {:error, :unauthorized} =
               CustomerAgentGrantStore.verify_credential(issued.token, binding, operation, 1_001)
    end
  end

  test "unavailable revocation lookup never permits an otherwise valid credential", %{
    binding: binding
  } do
    {:ok, issued} = CustomerAgentGrantStore.issue(binding, ["read_context"], 1_000, 60)
    key = "customer_agent_v1.revoked." <> issued.id
    Agent.update(CoreState, &%{&1 | failed_keys: [key]})
    before = Agent.get(CoreState, & &1.requests)

    assert {:error, :storage_unavailable} =
             CustomerAgentGrantStore.verify_credential(
               issued.token,
               binding,
               "read_context",
               1_001
             )

    assert Agent.get(CoreState, & &1.requests) == before + 2
  end

  test "any existing revocation marker denies access regardless of its value", %{binding: binding} do
    {:ok, issued} = CustomerAgentGrantStore.issue(binding, ["read_context"], 1_000, 60)
    key = "customer_agent_v1.revoked." <> issued.id

    for value <- [nil, false, %{}, %{"revoked" => false}] do
      Agent.update(CoreState, &put_in(&1, [:configs, key], value))

      assert {:error, :unauthorized} =
               CustomerAgentGrantStore.verify_credential(
                 issued.token,
                 binding,
                 "read_context",
                 1_001
               )
    end
  end

  test "expiry, tampered secrets, missing records and invalid input deny", %{binding: binding} do
    {:ok, issued} = CustomerAgentGrantStore.issue(binding, ["read_context"], 1_000, 60)

    assert {:error, :unauthorized} =
             CustomerAgentGrantStore.verify_credential(
               issued.token,
               binding,
               "read_context",
               1_060
             )

    assert {:error, :unauthorized} =
             CustomerAgentGrantStore.verify_credential(issued.token, binding, "read_context", 999)

    for token <- ["bad", String.duplicate("x", 1_000), issued.token <> "x"] do
      assert {:error, :unauthorized} =
               CustomerAgentGrantStore.verify_credential(token, binding, "read_context", 1_001)
    end

    last = if String.ends_with?(issued.token, "0"), do: "1", else: "0"
    tampered = binary_part(issued.token, 0, byte_size(issued.token) - 1) <> last

    assert {:error, :unauthorized} =
             CustomerAgentGrantStore.verify_credential(tampered, binding, "read_context", 1_001)

    assert {:error, :invalid_grant} =
             CustomerAgentGrantStore.issue(binding, ["approve"], 1_000, 60)

    assert {:error, :invalid_grant} =
             CustomerAgentGrantStore.issue(binding, ["read_context"], 1_000, 86_401)

    assert {:error, :invalid_grant} =
             CustomerAgentGrantStore.issue(
               Map.put(binding, "tenant_id", "../other"),
               ["read_context"],
               1_000,
               60
             )

    Agent.update(CoreState, &%{&1 | configs: %{}})

    assert {:error, :unauthorized} =
             CustomerAgentGrantStore.verify_credential(
               issued.token,
               binding,
               "read_context",
               1_001
             )
  end

  test "uses leader even when a stale follower is marked healthy", %{binding: binding} do
    {:ok, issued} = CustomerAgentGrantStore.issue(binding, ["read_context"], 1_000, 60)
    follower = "http://127.0.0.1:1"
    previous = Application.get_env(:query_service_ex, :core_read_urls)
    Application.put_env(:query_service_ex, :core_read_urls, [follower])
    :ets.insert(:core_node_health, {follower, :healthy, 0, "2026-09-26T12:00:00Z"})

    on_exit(fn ->
      :ets.delete(:core_node_health, follower)

      if previous,
        do: Application.put_env(:query_service_ex, :core_read_urls, previous),
        else: Application.delete_env(:query_service_ex, :core_read_urls)
    end)

    assert {:ok, _} =
             CustomerAgentGrantStore.verify_credential(
               issued.token,
               binding,
               "read_context",
               1_001
             )

    assert :ok = CustomerAgentGrantStore.revoke(binding, issued.id, 1_002)

    assert {:error, :unauthorized} =
             CustomerAgentGrantStore.verify_credential(
               issued.token,
               binding,
               "read_context",
               1_003
             )
  end

  test "opaque review credential cannot validate as a generic API-key JWT", %{binding: binding} do
    previous = System.get_env("JWT_SECRET")
    System.put_env("JWT_SECRET", "synthetic-grant-isolation-test-secret")

    on_exit(fn ->
      if previous,
        do: System.put_env("JWT_SECRET", previous),
        else: System.delete_env("JWT_SECRET")
    end)

    {:ok, issued} = CustomerAgentGrantStore.issue(binding, ["read_context"], 1_000, 60)
    assert {:error, :invalid_key} = RustCoreClient.decode_api_key_jwt(issued.token)
  end

  test "unavailable leader denies immediately, without retry or raw error disclosure", %{
    binding: binding
  } do
    {:ok, issued} = CustomerAgentGrantStore.issue(binding, ["read_context"], 1_000, 60)
    Agent.update(CoreState, &%{&1 | fail_read: true})
    before = Agent.get(CoreState, & &1.requests)

    assert {:error, :storage_unavailable} =
             CustomerAgentGrantStore.verify_credential(
               issued.token,
               binding,
               "read_context",
               1_001
             )

    assert Agent.get(CoreState, & &1.requests) == before + 1

    assert {:error, :invalid_tenant} = RustCoreClient.get_tenant_for_authorization("../other")
    assert Agent.get(CoreState, & &1.requests) == before + 1
  end

  test "failed durable writes never return a working credential or successful revoke", %{
    binding: binding
  } do
    {:ok, issued} = CustomerAgentGrantStore.issue(binding, ["read_context"], 1_000, 60)
    Agent.update(CoreState, &%{&1 | fail_write: true})

    assert {:error, :storage_unavailable} =
             CustomerAgentGrantStore.issue(binding, ["read_context"], 1_000, 60)

    assert {:error, :storage_unavailable} =
             CustomerAgentGrantStore.revoke(binding, issued.id, 1_001)
  end
end
