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
      metadata?(conn) -> conn
      conn.request_path == "/api/customer-agent/oauth/token" -> token_body(conn)
      content_type?(conn, "json") -> read(conn, limit(conn), &Jason.decode/1)
      true -> reject(conn, 415, "json_required")
    end
  end

  def call(conn, _opts), do: conn

  defp metadata?(conn),
    do:
      conn.method == "GET" and
        conn.request_path in [
          "/api/customer-agent/oauth/metadata",
          "/api/customer-agent/oauth/resource"
        ]

  defp content_type?(conn, subtype) do
    case get_req_header(conn, "content-type") do
      [type] -> match?({:ok, "application", ^subtype, _}, Utils.content_type(type))
      _ -> false
    end
  end

  defp token_body(conn) do
    if content_type?(conn, "x-www-form-urlencoded"),
      do: read(conn, 8_192, &decode_form/1),
      else: reject(conn, 415, "invalid_request")
  end

  defp limit(conn) do
    if String.starts_with?(conn.request_path, [
         "/api/customer-agent/connections/",
         "/api/customer-agent/oauth/"
       ]),
       do: 4_096,
       else: 65_536
  end

  defp read(conn, limit, decode) do
    case read_body(conn, length: limit, read_length: limit, read_timeout: 5_000) do
      {:ok, body, conn} ->
        case decode.(body) do
          {:ok, params} when is_map(params) -> %{conn | body_params: params}
          _ -> reject(conn, 400, "invalid_request")
        end

      {:more, _partial, conn} ->
        reject(conn, 413, "input_too_large")

      {:error, _} ->
        reject(conn, 400, "invalid_request")
    end
  end

  defp decode_form(body) do
    entries = URI.query_decoder(body) |> Enum.to_list()

    if length(entries) <= 8 and not Regex.match?(~r/%(?![0-9a-fA-F]{2})/, body) and
         Enum.all?(entries, fn {key, value} -> String.valid?(key) and String.valid?(value) end) and
         entries |> Enum.map(&elem(&1, 0)) |> Enum.uniq() |> length() == length(entries),
       do: {:ok, Map.new(entries)},
       else: {:error, :invalid_request}
  rescue
    _ -> {:error, :invalid_request}
  end

  defp reject(conn, status, code) do
    body =
      if String.starts_with?(conn.request_path, "/api/customer-agent/oauth/"),
        do: %{error: "invalid_request"},
        else: %{error: %{code: code}}

    conn
    |> put_resp_content_type("application/json")
    |> put_resp_header("cache-control", "no-store")
    |> send_resp(status, Jason.encode!(body))
    |> halt()
  end
end
