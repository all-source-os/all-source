defmodule QueryServiceExWeb.Plugs.ApiKeyAuth do
  @moduledoc """
  Plug that authenticates requests using customer API keys via the `X-API-Key` header.

  On success, sets:
  - `conn.assigns.tenant_id` — the tenant owning the key
  - `conn.assigns.current_tenant` — the full tenant struct
  - `conn.assigns.api_key` — the ApiKey record (for scope checks)
  - `conn.assigns.auth_method` — `:api_key`

  On failure, returns 401 with a JSON error body.

  Managed keys require current durable lifecycle metadata on every request.

  When `AUTH_DISABLED=true` (dev mode), this plug is bypassed — the upstream
  `AuthPipeline` + `TenantContext` plugs handle dev context injection.
  """

  import Plug.Conn

  alias QueryServiceEx.DevMode
  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient
  alias QueryServiceExWeb.Plugs.ManagedKeyAccess

  require Logger

  def init(opts), do: opts

  def call(conn, _opts) do
    if DevMode.auth_disabled?() do
      conn
    else
      authenticate(conn)
    end
  end

  defp authenticate(conn) do
    case get_req_header(conn, "x-api-key") |> List.first() do
      nil ->
        reject(conn, "missing X-API-Key header")

      "" ->
        reject(conn, "empty X-API-Key header")

      raw_key ->
        verify_key(conn, raw_key)
    end
  end

  defp verify_key(conn, raw_key) do
    case RustCoreClient.verify_api_key(raw_key) do
      {:ok, {tenant, api_key}} ->
        tid = tenant["id"] || tenant[:id]

        if subscription_active?(tenant) do
          conn
          |> assign(:tenant_id, tid)
          |> assign(:current_tenant, tenant)
          |> assign(:api_key, api_key)
          |> assign(:auth_method, :api_key)
          |> put_private(:allsource_tenant_id, tid)
          |> ManagedKeyAccess.key(api_key)
        else
          reject_subscription(conn, tenant)
        end

      {:error, :invalid_key} ->
        reject(conn, "invalid API key")

      {:error, :key_expired} ->
        reject(conn, "API key has expired")
    end
  end

  defp reject(conn, message) do
    Logger.warning("[ApiKeyAuth] Rejected: #{message}")

    conn
    |> put_status(:unauthorized)
    |> Phoenix.Controller.json(%{
      error: %{
        code: "unauthorized",
        message: message
      }
    })
    |> halt()
  end

  defp reject_subscription(conn, tenant) do
    status = get_subscription_status(tenant)

    conn
    |> put_status(:payment_required)
    |> Phoenix.Controller.json(%{
      error: %{
        code: "subscription_required",
        message: "Your subscription has #{status}. Please update your billing.",
        subscription_status: status
      }
    })
    |> halt()
  end

  # Subscription status helpers that work with both Ecto Tenant structs and Core maps

  defp subscription_active?(tenant) do
    get_subscription_status(tenant) in (~w(trialing active)a ++ ["trialing", "active"])
  end

  defp get_subscription_status(%{subscription_status: status}) when not is_nil(status), do: status

  defp get_subscription_status(tenant) when is_map(tenant) do
    get_in(tenant, ["metadata", "subscription", "status"]) ||
      get_in(tenant, [:metadata, :subscription, :status]) ||
      tenant["status"] ||
      "active"
  end
end
