defmodule McpServerElixir.Infrastructure.CustomerRemoteClient do
  @moduledoc "Remote bearer forwards only to the configured Query Service; no Core or session credential."

  def call(operation, arguments, credential) when operation in ["context", "validate"] do
    url = Application.get_env(:mcp_server_elixir, :customer_review_query_url)

    with true <- valid_url?(url),
         true <- is_binary(credential) and byte_size(credential) in 1..3_800,
         {:ok, body} <- Jason.encode(arguments),
         true <- byte_size(body) <= 65_536 do
      client =
        Tesla.client(
          [
            {Tesla.Middleware.BaseUrl, url},
            {Tesla.Middleware.Headers,
             [{"authorization", "Bearer " <> credential}, {"content-type", "application/json"}]},
            {Tesla.Middleware.Timeout, timeout: 65_000}
          ],
          Tesla.Adapter.Hackney
        )

      case Tesla.post(client, "/api/customer-agent/remote/" <> operation, body) do
        {:ok, %{status: 200, body: response}}
        when is_binary(response) and byte_size(response) <= 8_192 ->
          decode(response)

        {:ok, %{status: status}} when status in [401, 403] ->
          {:error, :access_denied}

        {:ok, %{status: 422}} ->
          {:error, :invalid_proposal}

        {:ok, %{status: 429}} ->
          {:error, :rate_limited}

        _ ->
          {:error, :access_unavailable}
      end
    else
      _ -> {:error, :access_unavailable}
    end
  rescue
    _ -> {:error, :access_unavailable}
  end

  defp decode(response) do
    case Jason.decode(response) do
      {:ok, %{"data" => %{"state" => state} = data}}
      when state in ["eligibility_verified", "valid_unresolved"] ->
        {:ok, data}

      _ ->
        {:error, :access_unavailable}
    end
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
