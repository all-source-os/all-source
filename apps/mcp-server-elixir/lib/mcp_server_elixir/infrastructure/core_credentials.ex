defmodule McpServerElixir.Infrastructure.CoreCredentials do
  @moduledoc """
  The `Authorization` value sent to Core or the hosted gateway.

  With `:client_assertion` configured (`ALLSOURCE_CLIENT_SIGNING_KEY`), every
  call returns a current ES256 assertion from `ClientAssertion` and the static
  `:core_api_key` is ignored. Otherwise the static key is sent with the
  `Bearer` scheme, which a hosted gateway requires (#228).

  As a Tesla middleware it runs inside `Tesla.Middleware.Retry`, so every
  attempt, retries included, asks for the credential again.
  """

  @behaviour Tesla.Middleware

  require Logger

  alias McpServerElixir.Infrastructure.ClientAssertion

  @doc "The Authorization header value, `:none` when nothing is configured."
  @spec authorization() :: {:ok, String.t()} | :none | {:error, term()}
  def authorization do
    if Application.get_env(:mcp_server_elixir, :client_assertion) do
      with {:ok, token} <- ClientAssertion.bearer(), do: {:ok, "Bearer " <> token}
    else
      static_key(Application.get_env(:mcp_server_elixir, :core_api_key))
    end
  end

  @doc "The Authorization header under `name`, or `[]` when there is none to send."
  @spec headers(String.t()) :: [{String.t(), String.t()}]
  def headers(name) do
    case authorization() do
      {:ok, value} ->
        [{name, value}]

      :none ->
        []

      {:error, reason} ->
        Logger.error("[CoreCredentials] no client assertion: #{inspect(reason)}")
        []
    end
  end

  @impl Tesla.Middleware
  def call(env, next, _opts) do
    case authorization() do
      {:ok, value} -> env |> Tesla.put_header("authorization", value) |> Tesla.run(next)
      :none -> Tesla.run(env, next)
      {:error, reason} -> {:error, {:client_assertion, reason}}
    end
  end

  defp static_key(nil), do: :none
  defp static_key(""), do: :none
  defp static_key("Bearer " <> _ = key), do: {:ok, key}
  defp static_key(key), do: {:ok, "Bearer " <> key}
end
