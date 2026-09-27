defmodule QueryServiceEx.Domain.CustomerAgent.RemoteAuthorization do
  @moduledoc "Exact pre-registered hosted-client and S256 request rules; no I/O or authority."
  alias QueryServiceEx.Domain.CustomerAgent.ConnectionGrant

  @client "claude-ai"
  @redirect "https://claude.ai/api/mcp/auth_callback"
  @scope "allsource.review"
  @fixed_request %{
    "client_id" => @client,
    "redirect_uri" => @redirect,
    "response_type" => "code",
    "scope" => @scope,
    "code_challenge_method" => "S256"
  }
  @exchange_keys ~w(client_id code code_verifier grant_type redirect_uri resource)

  def client, do: @client
  def redirect, do: @redirect
  def scope, do: @scope

  @spec validate_request(term(), term()) :: {:ok, map()} | {:error, :invalid_request}
  def validate_request(params, resource) when is_map(params) do
    if Map.drop(params, ~w(code_challenge state)) == Map.put(@fixed_request, "resource", resource) and
         ConnectionGrant.valid_resource?(resource) and
         challenge?(params["code_challenge"]) and state?(params["state"]) do
      {:ok, params}
    else
      {:error, :invalid_request}
    end
  end

  def validate_request(_, _), do: {:error, :invalid_request}

  @spec validate_exchange(term(), map(), term(), integer()) :: :ok | {:error, :invalid_grant}
  def validate_exchange(params, payload, resource, now) when is_map(params) and is_map(payload) do
    request = payload["request"]

    with true <- Enum.sort(Map.keys(params)) == @exchange_keys,
         true <- params["grant_type"] == "authorization_code",
         {:ok, _} <- validate_request(request, resource),
         true <- params["client_id"] == request["client_id"],
         true <- params["redirect_uri"] == request["redirect_uri"],
         true <- params["resource"] == request["resource"],
         true <- verifier?(params["code_verifier"]),
         true <- is_integer(payload["created_at"]) and is_integer(now),
         true <- now >= payload["created_at"] and now < payload["created_at"] + 300,
         expected =
           :sha256 |> :crypto.hash(params["code_verifier"]) |> Base.url_encode64(padding: false),
         true <- Plug.Crypto.secure_compare(expected, request["code_challenge"]) do
      :ok
    else
      _ -> {:error, :invalid_grant}
    end
  end

  def validate_exchange(_, _, _, _), do: {:error, :invalid_grant}

  defp challenge?(value) when is_binary(value) and byte_size(value) == 43 do
    case Base.url_decode64(value, padding: false) do
      {:ok, bytes} when byte_size(bytes) == 32 ->
        Base.url_encode64(bytes, padding: false) == value

      _ ->
        false
    end
  end

  defp challenge?(_), do: false

  defp verifier?(value) when is_binary(value) and byte_size(value) in 43..128,
    do: Regex.match?(~r/\A[A-Za-z0-9._~-]+\z/, value)

  defp verifier?(_), do: false

  defp state?(value) when is_binary(value) and byte_size(value) in 1..512,
    do: Regex.match?(~r/\A[\x21-\x7e]+\z/, value)

  defp state?(_), do: false
end
