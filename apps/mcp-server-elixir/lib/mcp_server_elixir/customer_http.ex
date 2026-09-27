defmodule McpServerElixir.CustomerHTTP do
  @moduledoc "Stateless JSON Streamable HTTP for the existing exclusive customer MCP profile."
  @behaviour Plug
  import Plug.Conn
  alias McpServerElixir.Infrastructure.CustomerRemoteClient
  alias McpServerElixir.Protocol.CustomerReview

  @impl true
  def init(opts), do: opts

  @impl true
  def call(conn, _opts) do
    cond do
      not Application.get_env(:mcp_server_elixir, :customer_review, false) ->
        respond(conn, 404, %{error: "unavailable"})

      conn.request_path != "/mcp/customer-review" ->
        respond(conn, 404, %{error: "not_found"})

      conn.query_string != "" or not valid_origin?(conn) ->
        respond(conn, 403, %{error: "access_denied"})

      true ->
        authorized(conn)
    end
  rescue
    _ -> respond(conn, 503, %{error: "unavailable"})
  end

  defp authorized(conn) do
    with ["Bearer " <> credential] <- get_req_header(conn, "authorization"),
         true <- byte_size(credential) in 1..3_800,
         {:ok, _} <- CustomerRemoteClient.call("context", %{}, credential) do
      dispatch(conn, credential)
    else
      {:error, :rate_limited} -> respond(conn, 429, %{error: "rate_limited"})
      {:error, :access_unavailable} -> respond(conn, 503, %{error: "unavailable"})
      _ -> challenge(conn)
    end
  end

  defp dispatch(%{method: "POST"} = conn, credential) do
    with true <-
           get_req_header(conn, "mcp-protocol-version") in [[], ["2025-06-18"], ["2025-11-25"]],
         [content_type] <- get_req_header(conn, "content-type"),
         {:ok, "application", "json", _} <- Plug.Conn.Utils.content_type(content_type) do
      read_request(conn, credential)
    else
      _ -> respond(conn, 400, %{error: "invalid_request"})
    end
  end

  defp dispatch(conn, _credential),
    do: conn |> put_resp_header("allow", "POST") |> respond(405, %{error: "method_not_allowed"})

  defp read_request(conn, credential) do
    case read_body(conn, length: 65_536, read_length: 65_536, read_timeout: 5_000) do
      {:ok, body, conn} ->
        respond_to_message(conn, body, credential)

      {:more, _, conn} ->
        respond(conn, 413, %{error: "input_too_large"})

      {:error, _} ->
        respond(conn, 400, %{error: "invalid_request"})
    end
  end

  defp respond_to_message(conn, body, credential) do
    result =
      CustomerReview.handle_line(body, fn operation, args ->
        case CustomerRemoteClient.call(operation, args, credential) do
          {:error, :access_denied} -> throw(:authorization_lost)
          result -> result
        end
      end)

    case result do
      nil -> conn |> private() |> send_resp(202, "")
      %{error: %{code: code}} when code in [-32_700, -32_600] -> respond(conn, 400, result)
      _ -> respond(conn, 200, result)
    end
  rescue
    _ -> respond(conn, 503, %{error: "unavailable"})
  catch
    :authorization_lost -> challenge(conn)
  end

  defp valid_origin?(conn) do
    case get_req_header(conn, "origin") do
      [] -> true
      [origin] -> origin in Application.get_env(:mcp_server_elixir, :customer_review_origins, [])
      _ -> false
    end
  end

  defp challenge(conn) do
    metadata = Application.get_env(:mcp_server_elixir, :customer_review_metadata_url)

    case is_binary(metadata) && URI.new(metadata) do
      {:ok, %URI{scheme: "https", host: host, userinfo: nil, query: nil, fragment: nil}}
      when is_binary(host) and host != "" ->
        conn
        |> put_resp_header(
          "www-authenticate",
          ~s(Bearer resource_metadata="#{metadata}", scope="allsource.review")
        )
        |> respond(401, %{error: "invalid_token"})

      _ ->
        respond(conn, 503, %{error: "unavailable"})
    end
  end

  defp private(conn),
    do:
      conn
      |> put_resp_header("cache-control", "no-store")
      |> put_resp_header("referrer-policy", "no-referrer")

  defp respond(conn, status, payload),
    do:
      conn
      |> private()
      |> put_resp_content_type("application/json")
      |> send_resp(status, Jason.encode!(payload))
end
