defmodule QueryServiceExWeb.Plugs.RequestLogger do
  @moduledoc """
  Plug for structured request/response logging.

  Logs incoming requests and outgoing responses with structured metadata including:
  - HTTP method and path
  - Response status code
  - Request duration in milliseconds
  - Client IP address
  - User agent
  - Request ID and correlation ID

  Designed for production observability with JSON-formatted logs.
  """

  require Logger

  @behaviour Plug

  @impl true
  def init(opts), do: opts

  @impl true
  def call(conn, _opts) do
    start_time = System.monotonic_time()

    Plug.Conn.register_before_send(conn, fn conn ->
      log_request(conn, start_time)
      conn
    end)
  end

  defp log_request(conn, start_time) do
    duration_ms = duration_ms(start_time)

    metadata = [
      method: conn.method,
      path: conn.request_path,
      status: conn.status,
      duration_ms: duration_ms,
      client_ip: client_ip(conn),
      user_agent: if(customer_review?(conn), do: nil, else: get_user_agent(conn)),
      query_string: if(customer_review?(conn), do: "", else: conn.query_string)
    ]

    Logger.metadata(metadata)

    level =
      if health_check_path?(conn.request_path) do
        :debug
      else
        status_to_level(conn.status)
      end

    Logger.log(
      level,
      fn ->
        "#{conn.method} #{conn.request_path} - #{conn.status} (#{duration_ms}ms)"
      end,
      metadata
    )
  end

  defp customer_review?(%{request_path: "/api/customer-agent/" <> _}), do: true
  defp customer_review?(_), do: false

  defp duration_ms(start_time) do
    stop_time = System.monotonic_time()
    System.convert_time_unit(stop_time - start_time, :native, :microsecond) / 1000
  end

  defp client_ip(conn) do
    # Check for X-Forwarded-For header first (for proxied requests)
    case Plug.Conn.get_req_header(conn, "x-forwarded-for") do
      [forwarded_for | _] ->
        forwarded_for
        |> String.split(",")
        |> List.first()
        |> String.trim()

      [] ->
        conn.remote_ip
        |> :inet.ntoa()
        |> to_string()
    end
  end

  defp get_user_agent(conn) do
    case Plug.Conn.get_req_header(conn, "user-agent") do
      [user_agent | _] -> user_agent
      [] -> nil
    end
  end

  defp health_check_path?("/api/health" <> _), do: true
  defp health_check_path?("/health" <> _), do: true
  defp health_check_path?(_), do: false

  defp status_to_level(status) when status >= 500, do: :error
  defp status_to_level(status) when status >= 400, do: :warning
  defp status_to_level(_status), do: :info
end
