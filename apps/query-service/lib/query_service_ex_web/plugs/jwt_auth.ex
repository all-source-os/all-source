defmodule QueryServiceExWeb.Plugs.JwtAuth do
  @moduledoc """
  Plug that validates shared-secret JWTs from the `Authorization: Bearer <token>` header.

  Uses HMAC-SHA256 with `JWT_SECRET` env var (shared with Core and Control Plane)
  for local signature validation. Managed JWTs also require current Core
  registration and permission for the requested operation.

  On success, sets:
  - `conn.assigns.tenant_id` — from the `tenant_id` claim
  - `conn.assigns.user_id` — from the `sub` claim
  - `conn.assigns.role` — from the `role` claim
  - `conn.assigns.auth_method` — `:jwt`

  On failure, returns 401 with a JSON error body.

  When `AUTH_DISABLED=true` (dev mode), this plug is bypassed.
  """

  import Plug.Conn

  alias QueryServiceEx.DevMode
  alias QueryServiceEx.Infrastructure.Adapters.ApiKeyAuthority
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
    with {:ok, token} <- extract_bearer_token(conn),
         {:ok, claims} <- verify_and_decode(token),
         {:ok, conn} <- ManagedKeyAccess.claims(conn, claims) do
      assign_claims(conn, claims)
      |> ManagedKeyAccess.guard_ingest()
    else
      {:error, message} ->
        reject(conn, message)
    end
  end

  defp extract_bearer_token(conn) do
    case get_req_header(conn, "authorization") |> List.first() do
      nil ->
        {:error, "missing Authorization header"}

      "Bearer " <> token when byte_size(token) > 0 ->
        {:ok, token}

      "Bearer " ->
        {:error, "empty Bearer token"}

      _ ->
        {:error, "invalid Authorization header format, expected Bearer <token>"}
    end
  end

  defp verify_and_decode(token) do
    case jwt_secret() do
      nil ->
        Logger.error("[JwtAuth] JWT_SECRET not configured")
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

  defp assign_claims(conn, claims) do
    conn
    |> assign(:tenant_id, claims["tenant_id"])
    |> assign(:user_id, claims["sub"])
    |> assign(:role, claims["role"])
    |> assign(:auth_method, :jwt)
    |> put_private(:allsource_tenant_id, claims["tenant_id"])
  end

  defp reject(conn, message) do
    Logger.warning("[JwtAuth] Rejected: #{message}")

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

  defp jwt_secret do
    System.get_env("JWT_SECRET")
  end
end
