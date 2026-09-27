defmodule QueryServiceExWeb.CustomerOAuthController do
  @moduledoc "Opt-in hosted connection handshake. OAuth consent never approves a product action."
  use Phoenix.Controller, formats: [:json]
  import Plug.Conn
  alias QueryServiceEx.Application.Services.CustomerRemoteAuthorization, as: Remote
  alias QueryServiceEx.Domain.CustomerAgent.ConnectionConsent
  alias QueryServiceEx.Domain.CustomerAgent.ConnectionGrant
  alias QueryServiceEx.Domain.CustomerAgent.RemoteAuthorization
  alias QueryServiceEx.RateLimiter
  alias QueryServiceExWeb.CustomerHumanSession

  def metadata(conn, params), do: dispatch(conn, params, :metadata)
  def resource(conn, params), do: dispatch(conn, params, :resource)
  def prepare(conn, params), do: dispatch(conn, params, :prepare)
  def inspect_request(conn, params), do: dispatch(conn, params, :inspect)
  def authorize(conn, params), do: dispatch(conn, params, :authorize)
  def token(conn, params), do: dispatch(conn, params, :token)

  defp dispatch(conn, params, operation) do
    with true <- Application.get_env(:query_service_ex, :customer_remote_enabled, false),
         true <- conn.query_string == "",
         {:ok, config} <- config(),
         {:allow, _} <- RateLimiter.check_rate("customer-oauth:admission", :free),
         {:ok, result} <- bounded(fn -> perform(operation, conn, params, config) end) do
      conn |> private() |> json(result)
    else
      false -> error(conn, 404, "unavailable")
      {:deny, _} -> error(conn, 429, "temporarily_unavailable")
      {:error, :connection_limit} -> error(conn, 429, "temporarily_unavailable")
      {:error, :storage_unavailable} -> error(conn, 503, "temporarily_unavailable")
      {:error, :access_denied} -> error(conn, 403, "access_denied")
      {:error, :invalid_grant} -> error(conn, 400, "invalid_grant")
      _ -> error(conn, 400, "invalid_request")
    end
  rescue
    _ -> error(conn, 503, "temporarily_unavailable")
  end

  defp perform(:metadata, _conn, params, config) when map_size(params) == 0 do
    {:ok,
     %{
       issuer: config.issuer,
       authorization_endpoint: config.issuer <> "/api/customer-agent/oauth/authorize",
       token_endpoint: config.issuer <> "/api/customer-agent/oauth/token",
       response_types_supported: ["code"],
       grant_types_supported: ["authorization_code"],
       token_endpoint_auth_methods_supported: ["none"],
       code_challenge_methods_supported: ["S256"],
       scopes_supported: [RemoteAuthorization.scope()],
       authorization_response_iss_parameter_supported: true
     }}
  end

  defp perform(:resource, _conn, params, config) when map_size(params) == 0 do
    {:ok,
     %{
       resource: config.resource,
       authorization_servers: [config.issuer],
       scopes_supported: [RemoteAuthorization.scope()],
       bearer_methods_supported: ["header"]
     }}
  end

  defp perform(:prepare, conn, params, config) do
    with [] <- get_req_header(conn, "authorization"),
         {:ok, request_token} <-
           Remote.seal_request(params, config.resource, now()) do
      {:ok, %{request_token: request_token}}
    else
      _ -> {:error, :invalid_request}
    end
  end

  defp perform(:inspect, conn, %{"request_token" => token} = params, config)
       when map_size(params) == 1 do
    with [] <- get_req_header(conn, "authorization"),
         {:ok, request} <- Remote.open_request(token, config.resource, now()) do
      {:ok,
       request
       |> Map.take(~w(client_id redirect_uri resource scope state))
       |> Map.put("issuer", config.issuer)}
    else
      _ -> {:error, :invalid_request}
    end
  end

  defp perform(
         :authorize,
         conn,
         %{"request_token" => token, "consent" => consent} = params,
         config
       )
       when map_size(params) == 2 do
    with true <- Application.get_env(:query_service_ex, :customer_connections_enabled, false),
         {:ok, actor} <- CustomerHumanSession.actor(conn),
         {:allow, _} <-
           RateLimiter.check_rate("customer-connections:" <> actor["tenant_id"], :free),
         {:ok, request} <- Remote.open_request(token, config.resource, now()),
         true <- consent == %{"accepted" => true, "version" => ConnectionConsent.version()},
         {:ok, issued} <- Remote.authorize(actor, request, consent, now()) do
      {:ok, Map.put(issued, :issuer, config.issuer)}
    else
      false -> {:error, :access_denied}
      error -> error
    end
  end

  defp perform(:token, conn, params, _config) do
    with [] <- get_req_header(conn, "authorization"),
         {:ok, issued} <- Remote.redeem(params, now()),
         stamp = now(),
         {:ok, access_token} <- Remote.seal_access(issued, stamp) do
      {:ok,
       %{
         access_token: access_token,
         token_type: "Bearer",
         expires_in: issued.expires_at - stamp,
         scope: issued.scope
       }}
    else
      {:error, :storage_unavailable} = error -> error
      _ -> {:error, :invalid_grant}
    end
  end

  defp perform(_, _, _, _), do: {:error, :invalid_request}

  defp config do
    issuer = Application.get_env(:query_service_ex, :customer_oauth_issuer)
    resource = Application.get_env(:query_service_ex, :customer_review_resource)

    if ConnectionGrant.valid_resource?(issuer) and ConnectionGrant.valid_resource?(resource) and
         URI.parse(issuer).path in [nil, ""],
       do: {:ok, %{issuer: issuer, resource: resource}},
       else: {:error, :storage_unavailable}
  end

  defp bounded(fun) do
    task =
      Task.async(fn ->
        try do
          fun.()
        rescue
          _ -> {:error, :storage_unavailable}
        catch
          _, _ -> {:error, :storage_unavailable}
        end
      end)

    # A lost token response requires reconnect; never repeat a consumed exchange.
    case Task.yield(task, 8_000) || Task.shutdown(task, :brutal_kill) do
      {:ok, result} -> result
      _ -> {:error, :storage_unavailable}
    end
  end

  defp private(conn),
    do:
      conn
      |> put_resp_header("cache-control", "no-store")
      |> put_resp_header("pragma", "no-cache")
      |> put_resp_header("referrer-policy", "no-referrer")

  defp error(conn, status, code),
    do: conn |> private() |> put_status(status) |> json(%{error: code})

  defp now, do: System.system_time(:second)
end
