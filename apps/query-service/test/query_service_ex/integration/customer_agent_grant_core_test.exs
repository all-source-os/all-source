defmodule QueryServiceEx.Integration.CustomerAgentGrantCoreTest do
  @moduledoc """
  Real Core admin boundary and WAL recovery for customer connection credentials.

  Run with ALLSOURCE_CORE_BINARY pointing at Core built with `--features enterprise` and
  `mix test --include integration <this file>`. Every Core process uses a private
  temporary directory, synthetic signing secret, loopback listener and no dev
  auth bypass. Processes are killed between phases, not cleanly checkpointed.
  """
  use ExUnit.Case, async: false

  alias QueryServiceEx.Infrastructure.Adapters.CustomerAgentGrantStore
  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient

  @moduletag :integration
  @moduletag timeout: 60_000
  @binary System.get_env("ALLSOURCE_CORE_BINARY")
  @moduletag skip: is_nil(@binary)
  @secret "synthetic-only-core-grant-recovery-secret-2026"

  setup do
    suffix = :crypto.strong_rand_bytes(8) |> Base.encode16(case: :lower)
    directory = Path.join(System.tmp_dir!(), "allsource-grant-core-#{suffix}")
    File.mkdir!(directory)
    File.chmod!(directory, 0o700)
    keys = [:core_url, :core_write_url, :core_read_urls, :core_api_key]
    previous = Enum.map(keys, &{&1, Application.get_env(:query_service_ex, &1)})
    port = free_port()
    url = "http://127.0.0.1:#{port}"
    Application.put_env(:query_service_ex, :core_url, url)
    Application.put_env(:query_service_ex, :core_write_url, url)
    Application.put_env(:query_service_ex, :core_read_urls, [url])
    Application.put_env(:query_service_ex, :core_api_key, "Bearer " <> token("admin"))

    on_exit(fn ->
      Enum.each(previous, fn
        {key, nil} -> Application.delete_env(:query_service_ex, key)
        {key, value} -> Application.put_env(:query_service_ex, key, value)
      end)

      File.rm_rf!(directory)
    end)

    %{directory: directory, port: port, url: url}
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

  defp with_core(context, fun) do
    executable = String.to_charlist(Path.expand(@binary))

    env =
      Enum.map(core_env(context), fn {key, value} ->
        {String.to_charlist(key), String.to_charlist(value)}
      end)

    # test-hang-allow: owned loopback child, bounded readiness; always SIGKILL and await exit.
    port =
      Port.open({:spawn_executable, executable}, [
        :binary,
        :exit_status,
        :stderr_to_stdout,
        {:cd, String.to_charlist(context.directory)},
        {:env, env}
      ])

    try do
      wait_ready(port, context.url, 30)
      fun.()
    after
      stop_core(port)
    end
  end

  defp core_env(context) do
    %{
      "ALLSOURCE_HOST" => "127.0.0.1",
      "ALLSOURCE_PORT" => to_string(context.port),
      "ALLSOURCE_DATA_DIR" => context.directory,
      "ALLSOURCE_SYSTEM_DATA_DIR" => Path.join(context.directory, "system"),
      "ALLSOURCE_JWT_SECRET" => @secret,
      "ALLSOURCE_DEV_MODE" => "false",
      "ALLSOURCE_AUTH_DISABLED" => "false",
      "ALLSOURCE_ROLE" => "leader",
      "ALLSOURCE_REPLICATION_ENABLED" => "false",
      "ALLSOURCE_CLUSTER_ENABLED" => "false",
      "ALLSOURCE_BOOTSTRAP_API_KEY" => "",
      "ALLSOURCE_BOOTSTRAP_TENANT" => "",
      "ALLSOURCE_RESP_PORT" => "",
      "RUST_LOG" => "error"
    }
  end

  defp wait_ready(_port, _url, 0),
    do: flunk("Owned Core process did not become ready within deadline")

  defp wait_ready(port, url, attempts) do
    drain_output(port, 100)
    client = Tesla.client([{Tesla.Middleware.Timeout, timeout: 300}], Tesla.Adapter.Hackney)

    case Tesla.get(client, url <> "/health") do
      {:ok, %{status: 200}} ->
        :ok

      _ ->
        # test-hang-allow: bounded readiness retry; at most 30 attempts.
        Process.sleep(100)
        wait_ready(port, url, attempts - 1)
    end
  end

  defp drain_output(_port, 0), do: :ok

  defp drain_output(port, remaining) do
    receive do
      {^port, {:data, _data}} -> drain_output(port, remaining - 1)
      {^port, {:exit_status, status}} -> flunk("Owned Core process exited with status #{status}")
    after
      0 -> :ok
    end
  end

  defp stop_core(port) do
    case Port.info(port, :os_pid) do
      {:os_pid, pid} ->
        System.cmd("/bin/kill", ["-KILL", to_string(pid)], stderr_to_stdout: true)
        await_exit(port, System.monotonic_time(:millisecond) + 3_000)

      nil ->
        :ok
    end
  end

  defp await_exit(port, deadline) do
    remaining = deadline - System.monotonic_time(:millisecond)

    if remaining <= 0, do: flunk("Owned Core process did not exit after SIGKILL")

    receive do
      {^port, {:exit_status, _status}} -> :ok
      {^port, {:data, _data}} -> await_exit(port, deadline)
    after
      remaining -> flunk("Owned Core process did not exit after SIGKILL")
    end
  end

  # Avoid printing Tesla.Env, which contains the synthetic Authorization header.
  defp response_status({:ok, %{status: status}}), do: status
  defp response_status(_), do: :request_failed

  defp token(role) do
    now = System.system_time(:second)

    claims = %{
      "sub" => "grant-test-user",
      "tenant_id" => "grant-test-tenant",
      "role" => role,
      "iss" => "allsource",
      "iat" => now,
      "exp" => now + 300
    }

    {_, jwt} =
      JOSE.JWT.sign(JOSE.JWK.from_oct(@secret), %{"alg" => "HS256"}, claims) |> JOSE.JWS.compact()

    jwt
  end

  defp free_port do
    # test-hang-allow: ephemeral reservation only; immediately closed, no accept loop.
    {:ok, socket} = :gen_tcp.listen(0, [:binary, active: false, ip: {127, 0, 0, 1}])
    {:ok, port} = :inet.port(socket)
    :ok = :gen_tcp.close(socket)
    port
  end
end
