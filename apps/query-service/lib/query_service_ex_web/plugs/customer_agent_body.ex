defmodule QueryServiceExWeb.Plugs.CustomerAgentBody do
  @moduledoc """
  Bounded JSON admission for customer review routes, before generic parsers.

  Returning an error with the updated connection preserves the HTTP adapter's
  body-read state. Raising from a custom body reader lost that state and closed
  the socket instead of returning a useful 413 in the real Bandit test.
  """

  import Plug.Conn
  alias Plug.Conn.Utils

  def init(opts), do: opts

  def call(%{request_path: "/api/customer-agent/" <> _} = conn, _opts) do
    cond do
      conn.query_string != "" -> reject(conn, 403, "access_denied")
      json?(conn) -> read(conn)
      true -> reject(conn, 415, "json_required")
    end
  end

  def call(conn, _opts), do: conn

  defp json?(conn) do
    case get_req_header(conn, "content-type") do
      [type] -> match?({:ok, "application", "json", _}, Utils.content_type(type))
      _ -> false
    end
  end

  defp read(conn) do
    case read_body(conn, length: 65_536, read_length: 65_536, read_timeout: 5_000) do
      {:ok, body, conn} ->
        case Jason.decode(body) do
          {:ok, params} when is_map(params) -> %{conn | body_params: params}
          _ -> reject(conn, 400, "invalid_request")
        end

      {:more, _partial, conn} ->
        reject(conn, 413, "input_too_large")

      {:error, _} ->
        reject(conn, 400, "invalid_request")
    end
  end

  defp reject(conn, status, code) do
    conn
    |> put_resp_content_type("application/json")
    |> put_resp_header("cache-control", "no-store")
    |> send_resp(status, Jason.encode!(%{error: %{code: code}}))
    |> halt()
  end
end
