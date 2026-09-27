defmodule McpServerElixir.Infrastructure.CustomerReviewClient do
  @moduledoc "Customer grants and bindings come only from the OS-owner-checked connection file."
  alias McpServerElixir.Infrastructure.CustomerConnectionFile
  alias McpServerElixir.Infrastructure.CustomerReviewHTTP

  def call(operation, arguments) when operation in ~w(context validate prepare review result) do
    with {:ok, %{"url" => url, "token" => token, "binding" => binding}} <-
           CustomerConnectionFile.load(),
         {:ok, encoded} <- Jason.encode(Map.put(arguments, "binding", binding)),
         true <- byte_size(encoded) <= 65_536 do
      CustomerReviewHTTP.post(
        String.trim_trailing(url, "/") <> "/api/customer-agent/" <> operation,
        token,
        encoded
      )
    else
      _ -> {:error, :invalid_connection}
    end
  rescue
    _ -> {:error, :access_unavailable}
  end

  def call(_, _), do: {:error, :unknown_operation}
end
