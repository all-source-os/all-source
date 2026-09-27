defmodule QueryServiceExWeb.CustomerConnectionsController do
  @moduledoc """
  Product-session connection settings. No generic JWT/dev-mode authentication.
  This consent surface cannot approve or execute a consequential agent proposal.
  """
  use Phoenix.Controller, formats: [:json]
  import Plug.Conn

  alias QueryServiceEx.Application.Services.CustomerConnections
  alias QueryServiceEx.Domain.CustomerAgent.ConnectionGrant
  alias QueryServiceEx.RateLimiter

  def index(conn, params), do: dispatch(conn, params, :list)
  def create(conn, params), do: dispatch(conn, params, :create)
  def revoke(conn, params), do: dispatch(conn, params, :revoke)

  defp dispatch(conn, params, operation) do
    with true <- Application.get_env(:query_service_ex, :customer_connections_enabled, false),
         true <- conn.query_string == "",
         {:allow, _} <- RateLimiter.check_rate("customer-connections:admission", :free),
         {:ok, actor} <- actor(conn),
         {:allow, _} <-
           RateLimiter.check_rate("customer-connections:" <> actor["tenant_id"], :free),
         {:ok, result} <- perform(operation, actor, params) do
      conn |> put_resp_header("cache-control", "no-store") |> json(%{data: result})
    else
      {:error, :connection_limit} ->
        error(conn, 429, "connection_limit")

      {:error, :storage_unavailable} ->
        error(conn, 503, "access_unavailable")

      {:error, code} when code in [:invalid_request, :invalid_grant, :invalid_consent] ->
        error(conn, 400, "invalid_request")

      {:deny, _} ->
        error(conn, 429, "rate_limited")

      _ ->
        error(conn, 403, "access_denied")
    end
  rescue
    _ -> error(conn, 503, "access_unavailable")
  end

  defp perform(:list, actor, params) when map_size(params) == 0,
    do: CustomerConnections.list(actor, System.system_time(:second))

  defp perform(:create, actor, params),
    do: CustomerConnections.create(actor, params, System.system_time(:second))

  defp perform(:revoke, actor, %{"id" => id} = params) when map_size(params) == 1,
    do: CustomerConnections.revoke(actor, id, System.system_time(:second))

  defp perform(_, _, _), do: {:error, :invalid_request}

  defp actor(conn) do
    with ["Bearer " <> token] <- get_req_header(conn, "authorization"),
         true <- byte_size(token) in 1..8_192,
         secret <- System.get_env("JWT_SECRET"),
         true <- is_binary(secret) and byte_size(secret) >= 32,
         {true, %JOSE.JWT{fields: claims}, _} <-
           JOSE.JWT.verify_strict(JOSE.JWK.from_oct(secret), ["HS256"], token),
         true <- valid_session?(claims) do
      {:ok, %{"tenant_id" => claims["tenant_id"], "subject_id" => claims["sub"]}}
    else
      _ -> {:error, :access_denied}
    end
  end

  defp valid_session?(claims) do
    ConnectionGrant.valid_id?(claims["tenant_id"]) and
      ConnectionGrant.valid_subject?(claims["sub"]) and
      claims["provider"] in ~w(google github email) and claims["email_verified"] == true and
      Enum.all?(~w(is_api_key is_demo view_as), &(claims[&1] in [nil, false])) and
      Enum.all?(~w(api_key core_api_key act_as), &(claims[&1] in [nil, ""])) and
      valid_lifetime?(claims, System.system_time(:second))
  end

  defp valid_lifetime?(claims, now) do
    is_integer(claims["exp"]) and claims["exp"] > now and
      is_integer(claims["iat"]) and claims["iat"] >= 0 and claims["iat"] <= now and
      (is_nil(claims["nbf"]) or (is_integer(claims["nbf"]) and claims["nbf"] <= now))
  end

  defp error(conn, status, code),
    do:
      conn
      |> put_resp_header("cache-control", "no-store")
      |> put_status(status)
      |> json(%{error: %{code: code}})
end
