defmodule McpServerElixir.Infrastructure.CustomerReviewClient do
  @moduledoc """
  Customer review profile's sole network boundary. It never holds a Core admin
  key, follows redirects, calls a general query endpoint or retries a request.
  Credentials and bindings come from an OS-owner-checked file, not tool arguments.
  """

  @spec call(String.t(), map()) :: {:ok, map()} | {:error, atom()}
  def call(operation, arguments) when operation in ["context", "validate"] do
    with {:ok, %{"url" => url, "token" => token, "binding" => binding}} <-
           McpServerElixir.Infrastructure.CustomerConnectionFile.load(),
         body = Map.put(arguments, "binding", binding),
         {:ok, encoded} <- Jason.encode(body),
         true <- byte_size(encoded) <= 65_536 do
      client =
        Tesla.client(
          [
            {Tesla.Middleware.BaseUrl, String.trim_trailing(url, "/")},
            {Tesla.Middleware.Headers,
             [
               {"authorization", "Bearer " <> token},
               {"content-type", "application/json"},
               {"accept", "application/json"}
             ]},
            {Tesla.Middleware.Timeout, timeout: 65_000}
          ],
          Tesla.Adapter.Hackney
        )

      case Tesla.post(client, "/api/customer-agent/" <> operation, encoded) do
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
      _ -> {:error, :invalid_connection}
    end
  rescue
    _ -> {:error, :access_unavailable}
  end

  def call(_, _), do: {:error, :unknown_operation}

  defp decode(response) do
    case Jason.decode(response) do
      {:ok, %{"data" => %{"state" => state} = data}}
      when state in ["eligibility_verified", "valid_unresolved"] ->
        {:ok, data}

      _ ->
        {:error, :access_unavailable}
    end
  end
end
