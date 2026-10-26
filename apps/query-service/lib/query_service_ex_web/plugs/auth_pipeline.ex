defmodule QueryServiceExWeb.Plugs.AuthPipeline do
  @moduledoc """
  Unified authentication pipeline for protected routes.

  Tries authentication methods in order:
  1. API key via `X-API-Key` header
  2. Shared-secret JWT via `Authorization: Bearer <token>` header
  3. Dev mode bypass when `AUTH_DISABLED=true`

  On success, sets `conn.assigns.current_user` (for JWT auth, built from claims)
  and `conn.assigns.auth_method` (`:api_key`, `:jwt`, or `:dev`).

  API key auth also sets `current_tenant`, `tenant_id`, `api_key` via ApiKeyAuth.
  JWT auth sets `user_id`, `tenant_id`, `role` and builds the user resource from claims.
  """

  @behaviour Plug

  import Plug.Conn

  alias QueryServiceEx.DevMode
  alias QueryServiceEx.Infrastructure.Adapters.ApiKeyAuthority
  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient
  alias QueryServiceExWeb.Plugs.ManagedKeyAccess

  require Logger

  def init(opts), do: opts

  def call(conn, _opts) do
    if DevMode.auth_disabled?() do
      bypass_auth(conn)
    else
      cond do
        has_api_key_header?(conn) ->
          authenticate_api_key(conn)

        has_bearer_token?(conn) ->
          authenticate_jwt(conn)

        true ->
          reject(conn, "missing authentication credentials")
      end
    end
    |> ManagedKeyAccess.guard_ingest()
  end

  # -------------------------------------------------------------------
  # API Key Authentication
  # -------------------------------------------------------------------

  defp has_api_key_header?(conn) do
    case get_req_header(conn, "x-api-key") |> List.first() do
      nil -> false
      "" -> false
      _ -> true
    end
  end

  defp authenticate_api_key(conn) do
    raw_key = get_req_header(conn, "x-api-key") |> List.first()

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

      {:error, :key_revoked} ->
        reject(conn, "API key has been revoked")

      {:error, :key_expired} ->
        reject(conn, "API key has expired")

      {:error, _reason} ->
        reject(conn, "API key verification failed")
    end
  end

  # -------------------------------------------------------------------
  # JWT Authentication
  # -------------------------------------------------------------------

  defp has_bearer_token?(conn) do
    case get_req_header(conn, "authorization") |> List.first() do
      "Bearer " <> token when byte_size(token) > 0 -> true
      _ -> false
    end
  end

  defp authenticate_jwt(conn) do
    with {:ok, token} <- extract_bearer_token(conn),
         {:ok, claims} <- verify_and_decode(token),
         {:ok, conn} <- ManagedKeyAccess.claims(conn, claims) do
      user = %{
        id: claims["sub"],
        email: claims["email"],
        name: claims["name"],
        tenant_id: claims["tenant_id"],
        role: claims["role"],
        provider: claims["provider"],
        is_demo: claims["is_demo"] == true,
        is_api_key: claims["is_api_key"] == true
      }

      conn
      |> assign(:current_user, user)
      |> assign(:tenant_id, claims["tenant_id"])
      |> assign(:user_id, claims["sub"])
      |> assign(:role, claims["role"])
      |> assign(:auth_method, :jwt)
      |> put_private(:allsource_tenant_id, claims["tenant_id"])
    else
      {:error, message} ->
        reject(conn, message)
    end
  end

  defp extract_bearer_token(conn) do
    case get_req_header(conn, "authorization") |> List.first() do
      "Bearer " <> token when byte_size(token) > 0 ->
        {:ok, token}

      "Bearer " ->
        {:error, "empty Bearer token"}

      nil ->
        {:error, "missing Authorization header"}

      _ ->
        {:error, "invalid Authorization header format"}
    end
  end

  defp verify_and_decode(token) do
    case jwt_secret() do
      nil ->
        Logger.error("[AuthPipeline] JWT_SECRET not configured")
        {:error, "JWT authentication not configured"}

      secret ->
        jwk = JOSE.JWK.from_oct(secret)

        case JOSE.JWT.verify_strict(jwk, ["HS256"], token) do
          {true, %JOSE.JWT{fields: claims}, _jws} ->
            validate_claims(claims)

          {false, _jwt, _jws} ->
            {:error, "invalid JWT signature"}

          {:error, _reason} ->
            {:error, "malformed JWT"}
        end
    end
  end

  defp validate_claims(claims) do
    now = System.system_time(:second)

    cond do
      not is_integer(claims["exp"]) and not ApiKeyAuthority.non_expiring_token?(claims) ->
        {:error, "missing exp claim"}

      is_integer(claims["exp"]) and claims["exp"] <= now ->
        {:error, "token has expired"}

      is_nil(claims["sub"]) ->
        {:error, "missing sub claim"}

      is_nil(claims["tenant_id"]) ->
        {:error, "missing tenant_id claim"}

      true ->
        {:ok, claims}
    end
  end

  # -------------------------------------------------------------------
  # Dev Mode Bypass
  # -------------------------------------------------------------------

  defp bypass_auth(conn) do
    dev_user = DevMode.dev_user()

    conn
    |> assign(:current_user, dev_user)
    |> assign(:tenant_id, dev_user.tenant_id)
    |> assign(:user_id, dev_user.id)
    |> assign(:auth_method, :dev)
    |> put_private(:allsource_dev_mode, true)
    |> put_private(:allsource_tenant_id, dev_user.tenant_id)
  end

  # -------------------------------------------------------------------
  # Error Responses
  # -------------------------------------------------------------------

  defp reject(conn, message) do
    Logger.warning("[AuthPipeline] Rejected: #{message}")

    conn
    |> put_status(:unauthorized)
    |> Phoenix.Controller.json(%{
      error: %{
        code: "unauthorized",
        message: message,
        correlation_id: conn.assigns[:correlation_id] || "unknown",
        timestamp: DateTime.utc_now() |> DateTime.to_iso8601()
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

  # Subscription status helpers that work with Core maps

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

  defp jwt_secret do
    System.get_env("JWT_SECRET")
  end
end
