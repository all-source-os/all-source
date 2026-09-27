defmodule McpServerElixir.Server do
  @moduledoc """
  Main MCP server process that handles JSON-RPC 2.0 messages over stdio.

  This GenServer reads from stdin and writes to stdout, processing MCP protocol
  messages and routing them to appropriate handlers.
  """

  use GenServer
  require Logger

  alias McpServerElixir.Infrastructure.ControlPlaneClient
  alias McpServerElixir.Protocol.{JsonRpc, McpTools}

  # Reported to every client in the `initialize` handshake — the only version
  # signal an MCP client gets. Read from the project version at compile time so
  # it cannot drift (it was hardcoded "0.1.0" while the app shipped 0.22.x, so
  # clients had no way to tell which build they were talking to).
  @server_info %{
    name: "allsource-mcp-elixir",
    version: Mix.Project.config()[:version]
  }

  def start_link(_opts) do
    GenServer.start_link(__MODULE__, [], name: __MODULE__)
  end

  @impl true
  def init(_opts) do
    # Log to stderr (MCP protocol uses stdout for JSON-RPC)
    :ok = :io.setopts(:standard_error, encoding: :utf8)

    :ok = :io.setopts(:standard_io, encoding: :utf8)

    Logger.info("🌟 AllSource MCP Server (Elixir) starting...")
    customer_review = Application.get_env(:mcp_server_elixir, :customer_review, false)

    Logger.info(
      "📡 Core API: #{Application.get_env(:mcp_server_elixir, :core_url, "http://localhost:3900")}"
    )

    Logger.info(
      "🎛️  Control Plane: #{Application.get_env(:mcp_server_elixir, :control_url, "http://localhost:3901")}"
    )

    # Start reading from stdin using a Task
    {:ok, _pid} = Task.start_link(fn -> stdin_reader_loop() end)

    read_only = customer_review or Application.get_env(:mcp_server_elixir, :read_only, false)

    # Accept both spellings, matching the CONTROL_URL || ALLSOURCE_CONTROL_URL
    # pair in config/runtime.exs. Reading only the ALLSOURCE_ form here meant
    # setting CONTROL_URL configured the client but left the tenant tools hidden.
    control_plane_enabled =
      not customer_review and
        (System.get_env("ALLSOURCE_CONTROL_URL") != nil or System.get_env("CONTROL_URL") != nil)

    # System-admin mode gates the mutating fleet recovery tools. OFF by default:
    # a connected MCP client can READ fleet health but cannot run a Destructive
    # recovery unless the operator explicitly enables it on this server instance.
    system_admin = not customer_review and System.get_env("ALLSOURCE_SYSTEM_ADMIN") == "true"

    if read_only, do: Logger.info("🔒 Read-only mode enabled")

    unless control_plane_enabled or customer_review,
      do:
        Logger.info(
          "ℹ️  Control plane not configured (set ALLSOURCE_CONTROL_URL to enable tenant tools)"
        )

    if customer_review,
      do: Logger.info("Customer review profile: restricted tools, no approval or execution")

    if system_admin,
      do:
        Logger.info(
          "🛡️  System-admin mode enabled — fleet recovery tools available (ALLSOURCE_SYSTEM_ADMIN=true)"
        )

    backend =
      if customer_review do
        nil
      else
        Application.get_env(
          :mcp_server_elixir,
          :core_backend,
          McpServerElixir.Infrastructure.CoreClient
        )
      end

    {:ok,
     %{
       backend: backend,
       control_client: if(customer_review, do: nil, else: ControlPlaneClient.new()),
       read_only: read_only,
       control_plane_enabled: control_plane_enabled,
       system_admin: system_admin,
       customer_review: customer_review
     }}
  end

  @impl true
  def handle_info({:stdin_line, line}, state) do
    handle_input(line, state)
    {:noreply, state}
  rescue
    # Last resort. `handle_input/2` already guards request handling with a
    # -32603 reply (see `guarded_process/2`); this clause only exists so that a
    # failure in the guard itself — or in parsing, or in the error path — still
    # cannot take the GenServer down. Nothing above this line may propagate.
    exception ->
      Logger.error(
        "stdio line handling raised: " <> Exception.format(:error, exception, __STACKTRACE__)
      )

      {:noreply, state}
  catch
    kind, value ->
      Logger.error("stdio line handling #{kind}: #{inspect(value)}")
      {:noreply, state}
  end

  @impl true
  def handle_info({:stdin_eof}, state) do
    Logger.info("EOF received, shutting down")
    {:stop, :normal, state}
  end

  defp stdin_reader_loop do
    case IO.read(:stdio, :line) do
      :eof ->
        send(__MODULE__, {:stdin_eof})

      {:error, reason} ->
        Logger.error("Error reading stdin: #{inspect(reason)}")
        send(__MODULE__, {:stdin_eof})

      line when is_binary(line) ->
        send(__MODULE__, {:stdin_line, line})
        stdin_reader_loop()
    end
  end

  defp handle_input(line, %{customer_review: true}) do
    case McpServerElixir.Protocol.CustomerReview.handle_line(line) do
      nil -> :ok
      response -> send_response(response)
    end
  end

  defp handle_input(line, state) do
    line
    |> String.trim()
    |> case do
      "" ->
        :ok

      json_string ->
        case JsonRpc.parse_request(json_string) do
          {:ok, request} ->
            guarded_process(request, state)

          {:error, reason} ->
            Logger.warning("Failed to parse JSON-RPC request: #{inspect(reason)}")
            send_error(nil, -32_700, "Parse error", nil)
        end
    end
  end

  # The barrier between client-controlled input and the GenServer's life.
  #
  # This has to sit at the seam — around the whole of `process_request/2` — not
  # deeper inside a tool. `McpTools.call_tool/3` shapes the arguments
  # (`Map.get(args, "format")`) before its own rescue is reachable, and
  # `process_request/2` reads `params["name"]` before that, so a client sending
  # `"arguments"` as a JSON string or a list — which real MCP/LLM clients
  # routinely do — raised a BadMapError straight out of `handle_info/2` and
  # killed the server. From the client's side that is the session dropping and
  # the tools "vanishing" with no config change (#229). With the supervisor at
  # the default `max_restarts: 3` in 5s, four such requests in five seconds
  # terminate the whole application.
  #
  # Everything a request touches is inside this guard: argument shaping,
  # dispatch, and the JSON encoding of the reply. A bad request degrades to a
  # JSON-RPC error; it never becomes a process exit.
  defp guarded_process(request, state) do
    process_request(request, state)
  rescue
    exception ->
      Logger.error(
        "Request #{inspect(Map.get(request, :method))} raised: " <>
          Exception.format(:error, exception, __STACKTRACE__)
      )

      fail_request(request, Exception.message(exception))
  catch
    kind, value ->
      Logger.error("Request #{inspect(Map.get(request, :method))} #{kind}: #{inspect(value)}")
      fail_request(request, "#{kind}: #{inspect(value)}")
  end

  # A notification (no id) is owed no response — but the process still lives.
  defp fail_request(%{id: nil}, _reason), do: :ok

  defp fail_request(%{id: id} = request, reason) do
    send_error(id, -32_603, error_message(failure_label(request), reason), reason)
  end

  # Keep per-tool attribution at the seam: a client that broke `tools/call`
  # wants to know *which* tool failed, not just that `tools/call` did.
  defp failure_label(%{method: "tools/call", params: params}) when is_map(params) do
    case Map.get(params, "name") do
      name when is_binary(name) -> name
      _ -> "tools/call"
    end
  end

  defp failure_label(%{method: method}) when is_binary(method), do: method
  defp failure_label(_request), do: "request"

  defp process_request(%{method: "initialize", params: _params, id: id}, state) do
    response = %{
      jsonrpc: "2.0",
      id: id,
      result: %{
        protocolVersion: "2024-11-05",
        capabilities: %{
          tools: %{}
        },
        serverInfo: @server_info
      }
    }

    respond(id, response)
    {:noreply, state}
  end

  defp process_request(%{method: "tools/list", id: id}, state) do
    config = %{
      read_only: Map.get(state, :read_only, false),
      control_plane_enabled: Map.get(state, :control_plane_enabled, false),
      system_admin: Map.get(state, :system_admin, false)
    }

    tools = McpTools.list_tools(config)

    response = %{
      jsonrpc: "2.0",
      id: id,
      result: %{
        tools: tools
      }
    }

    respond(id, response)
    {:noreply, state}
  end

  defp process_request(%{method: "tools/call", params: params, id: id}, state) do
    tool_name = Map.get(params, "name")
    args = Map.get(params, "arguments", %{})

    case McpTools.call_tool(tool_name, args, state) do
      {:ok, result} ->
        response = %{
          jsonrpc: "2.0",
          id: id,
          result: result
        }

        respond(id, response)

      {:error, reason} ->
        # Put the reason in `message`, not only in `data`. Clients surface
        # `message` and routinely drop `data`, so a bare "Internal error" told
        # the caller nothing about whether the tool was broken, the server was
        # down, or the store was wrong (#229).
        respond_error(id, -32_603, error_message(tool_name, reason), reason)
    end

    {:noreply, state}
  end

  defp process_request(%{method: "notifications/initialized"}, state) do
    # Handle initialized notification (no response needed)
    {:noreply, state}
  end

  defp process_request(%{method: method, id: id}, state)
       when is_binary(method) and not is_nil(id) do
    Logger.warning("Unknown method: #{method}")
    send_error(id, -32_601, "Method not found", nil)
    {:noreply, state}
  end

  defp process_request(%{method: method}, state) when is_binary(method) do
    # Unknown notification (no id = no response needed).
    #
    # This clause used to be unreachable: `JsonRpc.normalize_request/1` always
    # puts an `:id` key in the map (nil when the client sent none), so the
    # clause above matched every notification too and answered -32601 with
    # `"id": null` on stdout. MCP clients send `notifications/cancelled` and
    # `notifications/progress` as a matter of course, so a routine cancel put an
    # uncorrelatable error frame on the one stream the client has — the MCP SDK
    # reports that as "a response for an unknown message ID" (#229). Hence the
    # `not is_nil(id)` guard above.
    Logger.debug("Ignoring unknown notification: #{method}")
    {:noreply, state}
  end

  # JSON-RPC 2.0: "The Server MUST NOT reply to a Notification." MCP goes
  # further and forbids a null request id outright, so a nil id here can only
  # mean a notification — there is no client waiting on a correlation id, and
  # anything written would be unsolicited bytes on the protocol channel.
  #
  # `guarded_process/2`'s `fail_request/2` already honoured this for the crash
  # path; these two keep the success and tool-error paths consistent with it.
  defp respond(nil, _response), do: :ok
  defp respond(_id, response), do: send_response(response)

  defp respond_error(nil, _code, _message, _data), do: :ok
  defp respond_error(id, code, message, data), do: send_error(id, code, message, data)

  @doc false
  # Coerce an error reason into something Jason can encode. A raw reason is often
  # a struct (%Mint.TransportError{}, %Tesla.Env{}) or a tuple, and Jason raises
  # on those — inside this GenServer, which killed the server and dropped the
  # client's stdio connection instead of reporting the error (#229).
  def encodable(data) when is_binary(data), do: data
  def encodable(data) when is_number(data) or is_boolean(data) or is_nil(data), do: data
  def encodable(data) when is_atom(data), do: Atom.to_string(data)

  def encodable(data) when is_map(data) and not is_struct(data) do
    Map.new(data, fn {k, v} -> {encodable_key(k), encodable(v)} end)
  end

  def encodable(data) when is_list(data), do: Enum.map(data, &encodable/1)
  def encodable(data), do: inspect(data)

  defp encodable_key(k) when is_binary(k) or is_atom(k), do: k
  defp encodable_key(k), do: inspect(k)

  # Build a message that names the tool and carries the underlying reason, and
  # that stays a plain string so it always survives JSON encoding.
  @error_message_limit 500

  @doc false
  def error_message(tool_name, reason) do
    detail =
      case reason do
        r when is_binary(r) -> r
        r -> inspect(r)
      end

    "#{tool_name} failed: #{String.slice(detail, 0, @error_message_limit)}"
  end

  @unencodable_error ~s({"jsonrpc":"2.0","id":null,"error":{"code":-32603,) <>
                       ~s("message":"Internal error: response could not be serialized"}})

  @doc false
  # `Jason.encode!` here used to be the success path's own way of killing the
  # session: a result carrying anything Jason cannot encode raised inside the
  # GenServer, so the client lost its connection instead of getting an error
  # (#229). No shipping handler builds such a result today — this is the barrier
  # against the next one that does.
  #
  # Public (but @doc false) so the regression test can drive the real writer.
  def send_response(response) do
    case Jason.encode(response) do
      {:ok, json} ->
        write_line(json)

      {:error, reason} ->
        Logger.error(
          "Response for id #{inspect(Map.get(response, :id))} is not JSON-encodable: " <>
            inspect(reason)
        )

        write_line(unencodable_error(Map.get(response, :id)))
    end
  end

  # The fallback is built from an already-sanitised id and a literal message, so
  # it cannot fail the same way; the hardcoded constant is the floor if it does.
  defp unencodable_error(id) do
    payload = %{
      jsonrpc: "2.0",
      id: encodable(id),
      error: %{
        code: -32_603,
        message: "Internal error: response could not be serialized"
      }
    }

    case Jason.encode(payload) do
      {:ok, json} -> json
      {:error, _reason} -> @unencodable_error
    end
  end

  # MCP frames responses as JSON lines on stdout.
  defp write_line(json), do: IO.write(:stdio, json <> "\n")

  defp send_error(id, code, message, data) do
    error = %{
      code: code,
      message: message
    }

    error = if data, do: Map.put(error, :data, encodable(data)), else: error

    response = %{
      jsonrpc: "2.0",
      id: id,
      error: error
    }

    send_response(response)
  end
end
