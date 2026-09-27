defmodule QueryServiceEx.TestSupport.CustomerAgentCore do
  @moduledoc """
  Owned real-Core processes for customer review integration tests. Uses synthetic
  credentials, private temporary WALs and bounded lifecycle calls. No app source
  imports; callers supply an already-built binary through ALLSOURCE_CORE_BINARY.
  """
  import ExUnit.Assertions
  import ExUnit.Callbacks

  @secret "synthetic-only-core-grant-recovery-secret-2026"

  def setup_context do
    suffix = :crypto.strong_rand_bytes(8) |> Base.encode16(case: :lower)
    # Customer MCP fixtures require real paths, with no symlinked /var or /tmp.
    root = if :os.type() == {:unix, :darwin}, do: "/private/tmp", else: System.tmp_dir!()
    directory = Path.join(root, "allsource-grant-core-#{suffix}")
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

  def with_core(context, fun) do
    executable = String.to_charlist(Path.expand(System.fetch_env!("ALLSOURCE_CORE_BINARY")))

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
      wait_ready(port, context.url, 30, "")
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
      "ALLSOURCE_ARCHIVE_WORKERS" => "1",
      "ALLSOURCE_REPLICATION_ENABLED" => "false",
      "ALLSOURCE_CLUSTER_ENABLED" => "false",
      "ALLSOURCE_BOOTSTRAP_API_KEY" => "",
      "ALLSOURCE_BOOTSTRAP_TENANT" => "",
      "ALLSOURCE_RESP_PORT" => "",
      "RUST_LOG" => "info"
    }
  end

  defp wait_ready(port, _url, 0, output) do
    output = drain_output(port, 100, output)
    flunk("Owned Core process did not become ready within deadline. Child output:\n" <> output)
  end

  defp wait_ready(port, url, attempts, output) do
    output = drain_output(port, 100, output)
    client = Tesla.client([{Tesla.Middleware.Timeout, timeout: 300}], Tesla.Adapter.Hackney)

    case Tesla.get(client, url <> "/health") do
      {:ok, %{status: 200}} ->
        :ok

      _ ->
        # test-hang-allow: bounded readiness retry; at most 30 attempts.
        Process.sleep(100)
        wait_ready(port, url, attempts - 1, output)
    end
  end

  defp drain_output(_port, 0, output), do: output

  defp drain_output(port, remaining, output) do
    receive do
      {^port, {:data, data}} ->
        drain_output(port, remaining - 1, output_tail(output <> data))

      {^port, {:exit_status, status}} ->
        flunk("Owned Core process exited with status #{status}. Child output:\n" <> output)
    after
      0 -> output
    end
  end

  defp output_tail(output) when byte_size(output) <= 16_384, do: output
  defp output_tail(output), do: binary_part(output, byte_size(output) - 16_384, 16_384)

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

  def token(role) do
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
