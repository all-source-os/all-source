defmodule McpServerElixir.Infrastructure.CustomerConnectionFileTest do
  use ExUnit.Case, async: false

  alias McpServerElixir.Infrastructure.CustomerConnectionFile

  @key :customer_review_connection_file
  @config %{
    "version" => 1,
    "url" => "https://api.example.test",
    "token" => "asreview_v2_" <> String.duplicate("a", 32) <> "." <> String.duplicate("b", 64),
    "binding" => %{
      "tenant_id" => "test-tenant",
      "subject_id" => "oauth:google:123",
      "client_id" => "claude-code",
      "resource" => "https://api.example.test/customer-review"
    }
  }

  setup do
    # /var and /tmp are symlinks on macOS; the reader deliberately rejects them.
    root = if :os.type() == {:unix, :darwin}, do: "/private/tmp", else: System.tmp_dir!()
    directory = Path.join(root, "mcp-owner-#{System.unique_integer([:positive])}")
    File.mkdir!(directory)
    File.chmod!(directory, 0o700)
    path = Path.join(directory, "connection.json")
    previous = Application.get_env(:mcp_server_elixir, @key)
    Application.put_env(:mcp_server_elixir, @key, path)

    on_exit(fn ->
      Application.put_env(:mcp_server_elixir, @key, previous)
      File.rm_rf!(directory)
    end)

    %{path: path}
  end

  test "reads private connection through actual packaged reader and checks every call", %{
    path: path
  } do
    write(path, @config)
    assert {:ok, @config} = CustomerConnectionFile.load()
    File.chmod!(path, 0o644)
    assert {:error, :invalid_connection} = CustomerConnectionFile.load()
    File.chmod!(path, 0o600)
    assert {:ok, @config} = CustomerConnectionFile.load()
    File.rm!(path)
    assert {:error, :invalid_connection} = CustomerConnectionFile.load()
  end

  test "only declared local host and exact schema can load", %{path: path} do
    for config <- [
          Map.put(@config, "version", 2),
          Map.put(@config, "extra", "private-marker"),
          Map.put(@config, "token", "generic-human-session"),
          put_in(@config, ["binding", "client_id"], "claude-ai"),
          put_in(@config, ["binding", "tenant_id"], "../../other"),
          put_in(@config, ["binding", "extra"], true)
        ] do
      write(path, config)
      assert {:error, :invalid_connection} = CustomerConnectionFile.load()
    end
  end

  test "refuses secret redirection, plaintext remote destinations and URL credentials", %{
    path: path
  } do
    for url <- [
          "https://other.example.test",
          "https://api.example.test:444",
          "http://api.example.test",
          "http://localhost",
          "https://secret@api.example.test",
          "https://api.example.test/extra",
          "https://api.example.test?secret=marker",
          "https://api.example.test#marker"
        ] do
      write(path, Map.put(@config, "url", url))
      assert {:error, :invalid_connection} = CustomerConnectionFile.load()
    end

    write(path, Map.put(@config, "url", "http://127.0.0.1:4404"))
    assert {:ok, _} = CustomerConnectionFile.load()
  end

  test "old configuration cannot bypass required private file", %{path: path} do
    old = Application.get_env(:mcp_server_elixir, :customer_review_connection)
    Application.put_env(:mcp_server_elixir, :customer_review_connection, @config)
    on_exit(fn -> Application.put_env(:mcp_server_elixir, :customer_review_connection, old) end)
    assert {:error, :invalid_connection} = CustomerConnectionFile.load()
    File.write!(path, "malformed private-marker")
    File.chmod!(path, 0o600)
    assert {:error, :invalid_connection} = CustomerConnectionFile.load()
  end

  defp write(path, config) do
    File.write!(path, Jason.encode!(config))
    File.chmod!(path, 0o600)
  end
end
