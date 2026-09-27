defmodule QueryServiceEx.Infrastructure.Adapters.CustomerAuthorizationCode do
  @moduledoc "Encrypted five-minute OAuth codes. No secret or raw binding is persisted or logged."
  @behaviour QueryServiceEx.Domain.CustomerAgent.AuthorizationCodePort
  @salt "allsource-customer-authorization-code-v1"
  @keys ~w(binding created_at request token version)

  @impl true
  def available?, do: is_binary(secret()) and byte_size(secret()) >= 32

  @impl true
  def seal(payload, now) do
    if available?() and valid?(payload) do
      {:ok, Plug.Crypto.encrypt(secret(), @salt, payload, signed_at: now, max_age: 300)}
    else
      {:error, :storage_unavailable}
    end
  rescue
    _ -> {:error, :storage_unavailable}
  end

  @impl true
  def open(code) when is_binary(code) and byte_size(code) in 1..4_096 do
    with true <- available?(),
         {:ok, payload} <- Plug.Crypto.decrypt(secret(), @salt, code, max_age: 300),
         true <- valid?(payload) do
      {:ok, payload}
    else
      _ -> {:error, :invalid_grant}
    end
  rescue
    _ -> {:error, :invalid_grant}
  end

  def open(_), do: {:error, :invalid_grant}

  defp valid?(payload) when is_map(payload) do
    Enum.sort(Map.keys(payload)) == @keys and payload["version"] == 1 and
      is_map(payload["binding"]) and is_map(payload["request"]) and
      is_integer(payload["created_at"]) and payload["created_at"] >= 0 and
      is_binary(payload["token"]) and byte_size(payload["token"]) == 109
  end

  defp valid?(_), do: false
  defp secret, do: System.get_env("JWT_SECRET")
end
