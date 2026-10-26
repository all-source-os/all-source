defmodule QueryServiceExWeb.Plugs.JwksAuth do
  @moduledoc """
  Plug that validates JWTs using JWKS (JSON Web Key Set) from the Auth Service.

  Fetches the public keys from the Auth Service's `/.well-known/jwks.json`
  endpoint and caches them in an ETS table. Keys are refreshed:
  - Every 60 minutes (configurable via `AUTH_JWKS_REFRESH_SECONDS`)
  - Immediately on JWT validation failure (key rotation recovery)

  On success, sets:
  - `conn.assigns.tenant_id` — from the `tenant_id` claim (or "default" if missing)
  - `conn.assigns.user_id` — from the `sub` claim
  - `conn.assigns.role` — from the `role` claim (or "user" if missing)
  - `conn.assigns.auth_method` — `:jwks`

  Falls back to shared-secret JWT validation (JwtAuth) if JWKS is not configured
  or if JWKS validation fails but shared-secret succeeds.

  When `AUTH_DISABLED=true` (dev mode), this plug is bypassed.

  ## Configuration

  - `AUTH_JWKS_URL` — JWKS endpoint (e.g., `http://allsource-auth.internal:3903/.well-known/jwks.json`)
  - `AUTH_JWKS_REFRESH_SECONDS` — cache TTL in seconds (default: 3600)
  - `JWT_SECRET` — fallback shared secret for HMAC validation
  """

  import Plug.Conn

  alias QueryServiceEx.DevMode
  alias QueryServiceExWeb.Plugs.ManagedKeyAccess

  require Logger

  @ets_table :jwks_cache
  @ets_key :jwks_keys
  @ets_fetched_at :jwks_fetched_at

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
         {:ok, claims} <- verify_token(token),
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
        # Also check for session cookie (better-auth sends session tokens)
        case conn.cookies["better-auth.session_token"] do
          nil -> {:error, "missing Authorization header"}
          token -> {:ok, token}
        end

      "Bearer " <> token when byte_size(token) > 0 ->
        {:ok, token}

      _ ->
        {:error, "invalid Authorization header format"}
    end
  end

  defp verify_token(token) do
    # Try JWKS validation first
    case verify_with_jwks(token) do
      {:ok, claims} ->
        {:ok, claims}

      {:error, _jwks_error} ->
        # Fall back to shared-secret HMAC
        verify_with_shared_secret(token)
    end
  end

  # --- JWKS Validation ---

  defp verify_with_jwks(token) do
    case jwks_url() do
      nil ->
        {:error, "JWKS not configured"}

      url ->
        keys = get_cached_keys(url)

        case keys do
          [] ->
            {:error, "no JWKS keys available"}

          keys ->
            try_keys(token, keys, url)
        end
    end
  end

  defp try_keys(token, keys, url) do
    result =
      Enum.reduce_while(keys, {:error, "no matching key"}, fn key, _acc ->
        jwk = JOSE.JWK.from_map(key)

        case JOSE.JWT.verify(jwk, token) do
          {true, %JOSE.JWT{fields: claims}, _jws} ->
            {:halt, validate_claims(claims)}

          _ ->
            {:cont, {:error, "no matching key"}}
        end
      end)

    # If all keys failed, refresh and retry once
    case result do
      {:error, "no matching key"} ->
        refreshed_keys = fetch_and_cache_keys(url)

        Enum.reduce_while(refreshed_keys, {:error, "JWKS validation failed"}, fn key, _acc ->
          jwk = JOSE.JWK.from_map(key)

          case JOSE.JWT.verify(jwk, token) do
            {true, %JOSE.JWT{fields: claims}, _jws} ->
              {:halt, validate_claims(claims)}

            _ ->
              {:cont, {:error, "JWKS validation failed"}}
          end
        end)

      other ->
        other
    end
  end

  defp get_cached_keys(url) do
    ensure_ets_table()

    case :ets.lookup(@ets_table, @ets_key) do
      [{@ets_key, keys}] ->
        # Check if cache is stale
        case :ets.lookup(@ets_table, @ets_fetched_at) do
          [{@ets_fetched_at, fetched_at}] ->
            age = System.system_time(:second) - fetched_at

            if age > refresh_seconds() do
              fetch_and_cache_keys(url)
            else
              keys
            end

          _ ->
            fetch_and_cache_keys(url)
        end

      _ ->
        fetch_and_cache_keys(url)
    end
  end

  defp fetch_and_cache_keys(url) do
    Logger.info("[JwksAuth] Fetching JWKS from #{url}")

    case Req.get(url, receive_timeout: 5_000) do
      {:ok, %{status: 200, body: body}} ->
        keys = Map.get(body, "keys", [])
        ensure_ets_table()
        :ets.insert(@ets_table, {@ets_key, keys})
        :ets.insert(@ets_table, {@ets_fetched_at, System.system_time(:second)})
        Logger.info("[JwksAuth] Cached #{length(keys)} JWKS keys")
        keys

      {:ok, %{status: status}} ->
        Logger.error("[JwksAuth] JWKS fetch failed: HTTP #{status}")
        []

      {:error, error} ->
        Logger.error("[JwksAuth] JWKS fetch failed: #{inspect(error)}")
        []
    end
  end

  defp ensure_ets_table do
    case :ets.info(@ets_table) do
      :undefined ->
        :ets.new(@ets_table, [:set, :public, :named_table])

      _ ->
        :ok
    end
  end

  # --- Shared Secret Fallback ---

  defp verify_with_shared_secret(token) do
    case System.get_env("JWT_SECRET") do
      nil ->
        {:error, "no auth method available"}

      secret ->
        jwk = JOSE.JWK.from_oct(secret)

        case JOSE.JWT.verify_strict(jwk, ["HS256"], token) do
          {true, %JOSE.JWT{fields: claims}, _jws} ->
            validate_claims(claims)

          _ ->
            {:error, "invalid JWT"}
        end
    end
  end

  # --- Claims ---

  defp validate_claims(claims) do
    now = System.system_time(:second)

    cond do
      is_integer(claims["exp"]) and claims["exp"] <= now ->
        {:error, "token expired"}

      is_nil(claims["sub"]) ->
        {:error, "missing sub claim"}

      true ->
        {:ok, claims}
    end
  end

  defp assign_claims(conn, claims) do
    tenant_id = claims["tenant_id"] || "default"

    conn
    |> assign(:tenant_id, tenant_id)
    |> assign(:user_id, claims["sub"])
    |> assign(:role, claims["role"] || "user")
    |> assign(:auth_method, :jwks)
    |> put_private(:allsource_tenant_id, tenant_id)
  end

  defp reject(conn, message) do
    Logger.warning("[JwksAuth] Rejected: #{message}")

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

  # --- Config ---

  defp jwks_url do
    System.get_env("AUTH_JWKS_URL")
  end

  defp refresh_seconds do
    case System.get_env("AUTH_JWKS_REFRESH_SECONDS") do
      nil -> 3600
      val -> String.to_integer(val)
    end
  end
end
