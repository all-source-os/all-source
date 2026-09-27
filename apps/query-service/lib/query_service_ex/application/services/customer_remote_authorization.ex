defmodule QueryServiceEx.Application.Services.CustomerRemoteAuthorization do
  @moduledoc """
  Remote PKCE connection service, called only after browser actor/consent checks.

  The caller must establish a verified human session and CSRF protection before
  authorization. This internal service does not add a public route. Redemption
  returns an internal credential/binding for the HTTP transport's token envelope;
  it is neither a product session nor authority to approve a consequential action.
  """
  alias QueryServiceEx.Application.Services.CustomerAgentAccess
  alias QueryServiceEx.Application.Services.CustomerConnections
  alias QueryServiceEx.Domain.CustomerAgent.ConnectionConsent
  alias QueryServiceEx.Domain.CustomerAgent.ConnectionGrant
  alias QueryServiceEx.Domain.CustomerAgent.RemoteAuthorization

  @spec authorize(map(), map(), map(), integer()) :: {:ok, map()} | {:error, atom()}
  def authorize(actor, params, acceptance, now) do
    binding = CustomerConnections.binding(actor, RemoteAuthorization.client())

    with true <- ConnectionGrant.valid_binding?(binding),
         {:ok, request} <- RemoteAuthorization.validate_request(params, binding["resource"]),
         true <- codes().available?(),
         {:ok, _} <- CustomerConnections.eligible(binding, now),
         {:ok, issued} <-
           connections().issue(binding, ConnectionConsent.operations(), acceptance, now, 3_600) do
      finish_authorization(binding, request, issued, now)
    else
      false -> {:error, :invalid_request}
      error -> error
    end
  rescue
    _ -> {:error, :storage_unavailable}
  end

  @spec redeem(map(), integer()) :: {:ok, map()} | {:error, atom()}
  def redeem(params, now) when is_map(params) do
    started = System.monotonic_time(:millisecond)
    resource = Application.get_env(:query_service_ex, :customer_review_resource)

    with {:ok, payload} <- codes().open(params["code"]),
         :ok <- RemoteAuthorization.validate_exchange(params, payload, resource, now),
         binding = payload["binding"],
         true <- ConnectionGrant.valid_binding?(binding),
         true <- binding["client_id"] == RemoteAuthorization.client(),
         true <- binding["resource"] == resource,
         {:ok, _} <- CustomerConnections.eligible(binding, now),
         finished = now + div(System.monotonic_time(:millisecond) - started, 1_000),
         :ok <- RemoteAuthorization.validate_exchange(params, payload, resource, finished),
         :ok <- connections().activate_remote(payload["token"], binding, finished) do
      finish_redemption(payload, binding)
    else
      {:error, :storage_unavailable} = error -> error
      _ -> {:error, :invalid_grant}
    end
  rescue
    _ -> {:error, :storage_unavailable}
  end

  def redeem(_, _), do: {:error, :invalid_grant}

  defp finish_authorization(binding, request, issued, now) do
    payload = %{
      "version" => 1,
      "binding" => binding,
      "request" => request,
      "created_at" => now,
      "token" => issued.token
    }

    with {:ok, code} <- codes().seal(payload, now),
         {:ok, _} <- CustomerConnections.eligible(binding, System.system_time(:second)) do
      {:ok, %{code: code, redirect_uri: request["redirect_uri"], state: request["state"]}}
    else
      _ ->
        connections().revoke(binding, issued.id, System.system_time(:second))
        {:error, :storage_unavailable}
    end
  end

  defp finish_redemption(payload, binding) do
    case CustomerAgentAccess.verify(
           payload["token"],
           binding,
           "read_context",
           System.system_time(:second)
         ) do
      {:ok, context} ->
        {:ok,
         %{
           token: payload["token"],
           binding: binding,
           expires_at: context["grant_expires_at"],
           scope: RemoteAuthorization.scope()
         }}

      error ->
        error
    end
  end

  defp connections, do: Application.fetch_env!(:query_service_ex, :customer_connection_store)
  defp codes, do: Application.fetch_env!(:query_service_ex, :customer_authorization_code)
end
