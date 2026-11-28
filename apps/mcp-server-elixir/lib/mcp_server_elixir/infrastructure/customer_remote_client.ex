defmodule McpServerElixir.Infrastructure.CustomerRemoteClient do
  @moduledoc "Remote bearer forwards only to the configured Query Service; no Core or session credential."
  alias McpServerElixir.Infrastructure.CustomerReviewHTTP

  def call(operation, arguments, credential)
      when operation in ~w(session context validate prepare review result) do
    url = Application.get_env(:mcp_server_elixir, :customer_review_query_url)
    post(url, "/api/customer-agent/remote/" <> operation, arguments, credential, :review)
  end

  def call(_, _, _), do: {:error, :unknown_operation}

  def history(operation, arguments, credential) when operation in ~w(session timeline) do
    url = Application.get_env(:mcp_server_elixir, :event_history_query_url)
    post(url, "/api/event-history/" <> operation, arguments, credential, :history)
  end

  defp post(url, path, arguments, credential, profile) do
    with true <- valid_url?(url),
         true <- is_binary(credential) and byte_size(credential) in 1..3_800,
         {:ok, body} <- Jason.encode(arguments),
         true <- byte_size(body) <= 65_536 do
      CustomerReviewHTTP.post(
        String.trim_trailing(url, "/") <> path,
        credential,
        body,
        30_000,
        profile
      )
    else
      _ -> {:error, :access_unavailable}
    end
  rescue
    _ -> {:error, :access_unavailable}
  end

  defp valid_url?(url) when is_binary(url) and byte_size(url) <= 512 do
    case URI.new(url) do
      {:ok, %URI{scheme: scheme, host: host, userinfo: nil, query: nil, fragment: nil, path: path}}
      when path in [nil, "", "/"] ->
        (scheme == "https" and is_binary(host) and host != "") or
          (scheme == "http" and host in ["127.0.0.1", "::1"])

      _ ->
        false
    end
  end

  defp valid_url?(_), do: false
end
