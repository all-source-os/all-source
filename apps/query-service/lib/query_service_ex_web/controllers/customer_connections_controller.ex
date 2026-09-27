defmodule QueryServiceExWeb.CustomerConnectionsController do
  @moduledoc """
  Product-session connection settings. No generic JWT/dev-mode authentication.
  This consent surface cannot approve or execute a consequential agent proposal.
  """
  use Phoenix.Controller, formats: [:json]
  import Plug.Conn

  alias QueryServiceEx.Application.Services.CustomerConnections
  alias QueryServiceEx.RateLimiter
  alias QueryServiceExWeb.CustomerHumanSession

  def index(conn, params), do: dispatch(conn, params, :list)
  def create(conn, params), do: dispatch(conn, params, :create)
  def revoke(conn, params), do: dispatch(conn, params, :revoke)

  defp dispatch(conn, params, operation) do
    with true <- Application.get_env(:query_service_ex, :customer_connections_enabled, false),
         true <- conn.query_string == "",
         {:allow, _} <- RateLimiter.check_rate("customer-connections:admission", :free),
         {:ok, actor} <- CustomerHumanSession.actor(conn),
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

  defp error(conn, status, code),
    do:
      conn
      |> put_resp_header("cache-control", "no-store")
      |> put_status(status)
      |> json(%{error: %{code: code}})
end
