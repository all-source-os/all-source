defmodule QueryServiceExWeb.TenantController do
  @moduledoc """
  Controller for tenant management operations.

  Provides endpoints for viewing and managing tenant information,
  usage statistics, and settings. Tenant data is fetched from Core
  and cached locally via TenantCache.
  """

  use Phoenix.Controller, formats: [:json]
  use OpenApiSpex.ControllerSpecs

  alias QueryServiceEx.DevMode
  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient
  alias QueryServiceEx.TenantCache
  alias QueryServiceExWeb.Schemas.Common
  alias QueryServiceExWeb.Schemas.Tenant

  require Logger

  action_fallback(QueryServiceExWeb.FallbackController)

  tags(["Tenant"])

  operation(:show,
    summary: "Get tenant",
    description: "Returns the current user's tenant workspace information.",
    security: [%{"bearer_auth" => []}],
    responses: [
      ok: {"Tenant info", "application/json", Tenant.TenantResponse},
      not_found: {"Tenant not found", "application/json", Common.Error}
    ]
  )

  @doc """
  Returns the current user's tenant information.

  GET /api/tenant
  """
  def show(conn, _params) do
    if DevMode.auth_disabled?() do
      tenant = conn.assigns[:current_tenant] || DevMode.dev_tenant()

      conn
      |> put_status(:ok)
      |> json(%{data: serialize_tenant(tenant)})
    else
      user = conn.assigns[:current_user]
      tenant_id = user["tenant_id"] || user.tenant_id

      case fetch_tenant(tenant_id) do
        {:ok, tenant} ->
          conn
          |> put_status(:ok)
          |> json(%{data: serialize_tenant(tenant)})

        {:error, _reason} ->
          conn
          |> put_status(:not_found)
          |> json(%{error: %{code: "tenant_not_found", message: "Workspace not found"}})
      end
    end
  end

  operation(:update,
    summary: "Update tenant",
    description:
      "Updates the current user's tenant settings. Only name and settings can be updated.",
    security: [%{"bearer_auth" => []}],
    request_body:
      {"Tenant updates", "application/json", Tenant.UpdateTenantRequest, required: true},
    responses: [
      ok: {"Tenant updated", "application/json", Tenant.TenantResponse},
      unprocessable_entity: {"Validation error", "application/json", Common.Error}
    ]
  )

  @doc """
  Updates the current user's tenant settings.

  PUT /api/tenant

  Note: Tenant updates (name, settings) are now managed by the Control Plane.
  This endpoint returns the current cached tenant state. To update tenant
  settings, use the Control Plane's tenant update endpoint.
  """
  def update(conn, _params) do
    if DevMode.auth_disabled?() do
      # Dev/standalone mode: can't persist, return current tenant
      tenant = conn.assigns[:current_tenant] || DevMode.dev_tenant()

      conn
      |> put_status(:ok)
      |> json(%{data: serialize_tenant(tenant)})
    else
      user = conn.assigns[:current_user]
      tenant_id = user["tenant_id"] || user.tenant_id

      case fetch_tenant(tenant_id) do
        {:ok, tenant} ->
          conn
          |> put_status(:ok)
          |> json(%{data: serialize_tenant(tenant)})

        {:error, _reason} ->
          conn
          |> put_status(:not_found)
          |> json(%{error: %{code: "tenant_not_found", message: "Workspace not found"}})
      end
    end
  end

  operation(:usage,
    summary: "Get usage statistics",
    description: "Returns usage statistics (events and queries) for the current billing period.",
    security: [%{"bearer_auth" => []}],
    responses: [
      ok: {"Usage statistics", "application/json", Tenant.UsageResponse}
    ]
  )

  @doc """
  Returns usage statistics for the current tenant.

  GET /api/tenant/usage
  """
  def usage(conn, _params) do
    if DevMode.auth_disabled?() do
      tenant = conn.assigns[:current_tenant] || DevMode.dev_tenant()

      conn
      |> put_status(:ok)
      |> json(%{
        data: build_usage_response(tenant)
      })
    else
      user = conn.assigns[:current_user]
      tenant_id = user["tenant_id"] || user.tenant_id

      case fetch_tenant(tenant_id) do
        {:ok, tenant} ->
          conn
          |> put_status(:ok)
          |> json(%{data: build_usage_response(tenant)})

        {:error, _reason} ->
          conn
          |> put_status(:not_found)
          |> json(%{error: %{code: "tenant_not_found", message: "Workspace not found"}})
      end
    end
  end

  @valid_enforcement_modes ~w(permissive warn strict)

  @doc """
  Returns the current tenant's schema-enforcement mode.

  GET /api/tenant/schema-enforcement
  """
  def get_schema_enforcement(conn, _params) do
    case current_tenant_id(conn) do
      {:ok, tenant_id} ->
        mode =
          case RustCoreClient.get_tenant(tenant_id) do
            {:ok, tenant} when is_map(tenant) -> tenant["schema_enforcement"] || "permissive"
            _ -> "permissive"
          end

        json(conn, %{data: %{schema_enforcement: mode}})

      :dev ->
        # No persistent tenant in dev/standalone mode.
        json(conn, %{data: %{schema_enforcement: "permissive"}})
    end
  end

  @doc """
  Sets the current tenant's schema-enforcement mode.

  PUT /api/tenant/schema-enforcement  body: {"schema_enforcement": "strict"}
  """
  def set_schema_enforcement(conn, params) do
    mode = params["schema_enforcement"]

    if mode in @valid_enforcement_modes do
      case current_tenant_id(conn) do
        {:ok, tenant_id} ->
          case RustCoreClient.set_schema_enforcement(tenant_id, mode) do
            {:ok, _} ->
              TenantCache.invalidate(tenant_id)
              json(conn, %{data: %{schema_enforcement: mode}})

            {:error, :not_found} ->
              conn
              |> put_status(:not_found)
              |> json(%{error: %{code: "tenant_not_found", message: "Workspace not found"}})

            {:error, reason} ->
              conn
              |> put_status(:bad_gateway)
              |> json(%{error: %{code: "core_error", message: to_string(reason)}})
          end

        :dev ->
          # Dev/standalone mode can't persist — echo the requested mode.
          json(conn, %{data: %{schema_enforcement: mode}})
      end
    else
      conn
      |> put_status(:unprocessable_entity)
      |> json(%{
        error: %{
          code: "invalid_mode",
          message: "schema_enforcement must be one of: permissive, warn, strict"
        }
      })
    end
  end

  # -------------------------------------------------------------------
  # Private Helpers
  # -------------------------------------------------------------------

  # Resolves the caller's tenant id, mirroring show/2. Returns `:dev` when
  # auth is disabled (no persistent tenant to act on).
  defp current_tenant_id(conn) do
    if DevMode.auth_disabled?() do
      :dev
    else
      user = conn.assigns[:current_user]
      {:ok, user["tenant_id"] || user.tenant_id}
    end
  end

  @doc false
  def fetch_tenant(tenant_id) do
    TenantCache.fetch(tenant_id, fn ->
      case RustCoreClient.get_tenant(tenant_id) do
        {:ok, tenant} -> tenant
        {:error, _} -> nil
      end
    end)
    |> case do
      nil -> {:error, :not_found}
      tenant -> {:ok, tenant}
    end
  end

  defp serialize_tenant(tenant) when is_map(tenant) do
    # Handle Core-format tenant maps (string keys like "id", "name", etc.)
    metadata = tenant["metadata"] || %{}
    subscription = metadata["subscription"] || %{}
    quotas = metadata["quotas"] || %{}

    %{
      id: tenant["id"],
      name: tenant["name"],
      slug: tenant["slug"],
      status: tenant["status"],
      subscription_status: subscription["status"],
      subscription_tier: subscription["tier"],
      events_quota: quotas["events_quota"],
      queries_quota: quotas["queries_quota"],
      events_used: quotas["events_used"] || 0,
      queries_used: quotas["queries_used"] || 0,
      created_at: tenant["inserted_at"],
      updated_at: tenant["updated_at"]
    }
  end

  # credo:disable-for-next-line Credo.Check.Refactor.CyclomaticComplexity
  defp build_usage_response(tenant) when is_map(tenant) do
    metadata = tenant["metadata"] || %{}
    subscription = metadata["subscription"] || %{}
    quotas = metadata["quotas"] || %{}

    events_quota = quotas["events_quota"] || -1
    events_used = quotas["events_used"] || 0
    queries_quota = quotas["queries_quota"] || -1
    queries_used = quotas["queries_used"] || 0
    # The Control Plane omits a zero extraction allowance, so absent means none.
    extraction_quota = quotas["extraction_tokens_quota"] || 0
    extraction_used = quotas["extraction_tokens_used"] || 0

    %{
      tenant_id: tenant["id"],
      subscription_tier: subscription["tier"],
      subscription_status: subscription["status"],
      events: %{
        used: events_used,
        quota: events_quota,
        remaining: remaining(events_quota, events_used)
      },
      queries: %{
        used: queries_used,
        quota: queries_quota,
        remaining: remaining(queries_quota, queries_used)
      },
      extraction_tokens: %{
        used: extraction_used,
        quota: extraction_quota,
        remaining: remaining(extraction_quota, extraction_used)
      },
      billing_period: %{
        reset_at: nil
      }
    }
  end

  defp remaining(-1, _used), do: -1
  defp remaining(quota, used) when is_integer(quota), do: max(0, quota - used)
end
