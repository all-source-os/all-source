defmodule QueryServiceExWeb.ApiKeyController do
  @moduledoc """
  Controller for API key management.

  Provides CRUD operations for API keys. Keys are signed JWTs
  with `is_api_key: true` and an authoritative Core lifecycle registration.
  ETS holds only the dashboard metadata projection.
  """

  use Phoenix.Controller, formats: [:json]

  alias QueryServiceEx.ApiKeyStore
  alias QueryServiceEx.AuditLog
  alias QueryServiceEx.Infrastructure.Adapters.ApiKeyAuthority

  require Logger

  action_fallback(QueryServiceExWeb.FallbackController)

  @doc """
  Lists all API keys for the current tenant.

  GET /api/api-keys
  """
  def index(conn, _params) do
    tenant_id = get_tenant_id(conn)
    keys = ApiKeyStore.list_keys(tenant_id)

    serialized =
      Enum.map(keys, fn k ->
        %{
          id: k["id"],
          name: k["name"],
          description: k["description"],
          key_prefix: k["key_prefix"],
          scopes: k["scopes"] || [],
          tenant_id: k["tenant_id"],
          created_at: k["created_at"],
          expires_at: k["expires_at"],
          last_used_at: k["last_used"] || k["last_used_at"],
          active: k["active"] != false
        }
      end)

    conn
    |> put_status(:ok)
    |> json(%{keys: serialized, count: length(serialized)})
  end

  @doc """
  Creates a new API key.

  POST /api/api-keys
  Body: {"name": "My Key", "scopes": ["events:read"]}
  """
  def create(conn, params) do
    tenant_id = get_tenant_id(conn)
    user = conn.assigns[:current_user]
    name = params["name"] || "Untitled Key"

    key_id = generate_uuid()
    now = DateTime.utc_now() |> DateTime.to_iso8601()

    with true <- is_binary(name),
         true <- ApiKeyAuthority.valid_scopes?(params["scopes"]),
         {:ok, expires_at, exp} <- requested_expiration(params["expires_at"]),
         {:ok, secret} <- sign_api_key(key_id, tenant_id, name, params["scopes"], exp) do
      description = params["description"]
      scopes = params["scopes"]
      key_prefix = String.slice(secret, 0, 12)

      metadata = %{
        "id" => key_id,
        "name" => name,
        "description" => description,
        "key_prefix" => key_prefix,
        "scopes" => scopes,
        "tenant_id" => tenant_id,
        "created_by" => user && (user[:id] || user["id"]),
        "created_at" => now,
        "expires_at" => expires_at,
        "active" => true,
        "last_used" => nil
      }

      case ApiKeyStore.create_key(tenant_id, metadata) do
        {:ok, _} ->
          actor = (user && (user[:email] || user["email"])) || "unknown"

          Task.start(fn ->
            AuditLog.record(tenant_id, "api_key.created", actor, %{
              key_id: key_id,
              name: name,
              scopes: scopes
            })
          end)

          conn
          |> put_status(:created)
          |> json(%{
            id: key_id,
            name: name,
            description: description,
            key: secret,
            key_prefix: key_prefix,
            scopes: scopes,
            tenant_id: tenant_id,
            created_at: now,
            expires_at: expires_at,
            last_used_at: nil
          })

        {:error, _} ->
          unavailable(conn)
      end
    else
      false ->
        invalid_request(conn)

      {:error, :invalid_expiration} ->
        invalid_request(conn)

      {:error, reason} ->
        Logger.error("[ApiKeyController] Failed to sign API key: #{inspect(reason)}")

        conn
        |> put_status(:internal_server_error)
        |> json(%{error: %{code: "key_generation_failed", message: "Failed to create API key"}})
    end
  end

  @doc """
  Revokes an API key.

  DELETE /api/api-keys/:id
  """
  def revoke(conn, %{"id" => key_id}) do
    tenant_id = get_tenant_id(conn)
    user = conn.assigns[:current_user]

    case ApiKeyStore.revoke_key(tenant_id, key_id) do
      :ok ->
        actor = user[:email] || user["email"] || "unknown"

        Task.start(fn ->
          AuditLog.record(tenant_id, "api_key.revoked", actor, %{key_id: key_id})
        end)

        conn
        |> put_status(:ok)
        |> json(%{data: %{message: "API key revoked", id: key_id}})

      {:error, :not_found} ->
        conn
        |> put_status(:not_found)
        |> json(%{error: %{code: "key_not_found", message: "API key not found"}})

      {:error, _} ->
        unavailable(conn)
    end
  end

  # -------------------------------------------------------------------
  # Private Helpers
  # -------------------------------------------------------------------

  defp generate_uuid do
    import Bitwise
    <<a::32, b::16, c::16, d::16, e::48>> = :crypto.strong_rand_bytes(16)

    :io_lib.format(
      "~8.16.0b-~4.16.0b-~4.16.0b-~4.16.0b-~12.16.0b",
      [a, b, bor(band(c, 0x0FFF), 0x4000), bor(band(d, 0x3FFF), 0x8000), e]
    )
    |> IO.iodata_to_binary()
  end

  defp get_tenant_id(conn) do
    conn.assigns[:tenant_id] ||
      (conn.assigns[:current_user] &&
         (conn.assigns[:current_user][:tenant_id] || conn.assigns[:current_user]["tenant_id"]))
  end

  defp sign_api_key(key_id, tenant_id, name, scopes, exp) do
    case System.get_env("JWT_SECRET") do
      nil ->
        {:error, "JWT_SECRET not configured"}

      secret ->
        now = System.system_time(:second)

        claims = %{
          "sub" => key_id,
          "tenant_id" => tenant_id,
          "name" => name,
          # Must match the canonical role string every RBAC consumer enforces:
          # control-plane entities.RoleServiceAccount = "serviceaccount" and Core's
          # serde(rename_all = "lowercase") Role::ServiceAccount = "serviceaccount".
          # The underscore form ("service_account") falls through to no permissions,
          # so every minted key silently 403s on read AND write.
          "role" => "serviceaccount",
          "is_api_key" => true,
          "credential_kind" => "managed_api_key",
          "scopes" => scopes,
          "iss" => "allsource",
          "iat" => now
        }

        claims = if is_integer(exp), do: Map.put(claims, "exp", exp), else: claims

        jwk = JOSE.JWK.from_oct(secret)
        jws = %{"alg" => "HS256"}
        jwt = JOSE.JWT.from_map(claims)

        {_alg, token} = JOSE.JWT.sign(jwk, jws, jwt) |> JOSE.JWS.compact()
        {:ok, token}
    end
  end

  defp requested_expiration(nil) do
    {:ok, nil, nil}
  end

  defp requested_expiration(value) when is_binary(value) do
    with {:ok, expires, _} <- DateTime.from_iso8601(value),
         true <- DateTime.to_unix(expires) > System.system_time(:second) do
      expires = DateTime.truncate(expires, :second)
      {:ok, DateTime.to_iso8601(expires), DateTime.to_unix(expires)}
    else
      _ -> {:error, :invalid_expiration}
    end
  end

  defp requested_expiration(_), do: {:error, :invalid_expiration}

  defp invalid_request(conn),
    do:
      conn
      |> put_status(:bad_request)
      |> json(%{
        error: %{
          code: "invalid_key_options",
          message:
            "Enter a text name, choose a supported permission and a future expiration or no expiration"
        }
      })

  defp unavailable(conn),
    do:
      conn
      |> put_status(:service_unavailable)
      |> json(%{
        error: %{
          code: "key_store_unavailable",
          message: "Credential state could not be persisted"
        }
      })
end
