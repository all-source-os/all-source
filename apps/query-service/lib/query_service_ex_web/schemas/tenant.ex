defmodule QueryServiceExWeb.Schemas.Tenant do
  @moduledoc """
  OpenAPI schemas for tenant endpoints.
  """

  require OpenApiSpex

  alias OpenApiSpex.Schema

  defmodule TenantFull do
    @moduledoc "Full tenant entity"
    OpenApiSpex.schema(%{
      title: "TenantFull",
      description: "Full tenant workspace details",
      type: :object,
      properties: %{
        id: %Schema{type: :string, format: :uuid},
        name: %Schema{type: :string},
        slug: %Schema{type: :string},
        subscription_status: %Schema{
          type: :string,
          enum: ["trialing", "active", "past_due", "cancelled", "expired"]
        },
        subscription_tier: %Schema{
          type: :string,
          enum: ["free", "pro", "growth", "enterprise"]
        },
        trial_ends_at: %Schema{type: :string, format: :"date-time", nullable: true},
        subscription_ends_at: %Schema{type: :string, format: :"date-time", nullable: true},
        events_quota: %Schema{type: :integer, description: "Monthly event quota"},
        queries_quota: %Schema{type: :integer, description: "Monthly query quota"},
        events_used: %Schema{type: :integer, description: "Events used this period"},
        queries_used: %Schema{type: :integer, description: "Queries used this period"},
        settings: %Schema{type: :object, additionalProperties: true},
        created_at: %Schema{type: :string, format: :"date-time"},
        updated_at: %Schema{type: :string, format: :"date-time"}
      },
      required: [:id, :name, :slug],
      example: %{
        id: "987fcdeb-51a2-43e1-b2c4-123456789000",
        name: "Acme Corp",
        slug: "acme-corp",
        subscription_status: "active",
        subscription_tier: "growth",
        trial_ends_at: nil,
        events_quota: 1_000_000,
        queries_quota: 100_000,
        events_used: 50_000,
        queries_used: 5000,
        settings: %{timezone: "UTC"},
        created_at: "2026-01-01T00:00:00Z",
        updated_at: "2026-02-10T12:00:00Z"
      }
    })
  end

  defmodule UpdateTenantRequest do
    @moduledoc "Request to update tenant settings"
    OpenApiSpex.schema(%{
      title: "UpdateTenantRequest",
      description: "Request body for updating tenant settings",
      type: :object,
      properties: %{
        name: %Schema{type: :string, minLength: 1, maxLength: 255},
        settings: %Schema{type: :object, additionalProperties: true}
      },
      example: %{
        name: "Acme Corporation",
        settings: %{timezone: "America/New_York"}
      }
    })
  end

  defmodule TenantResponse do
    @moduledoc "Tenant response"
    OpenApiSpex.schema(%{
      title: "TenantResponse",
      description: "Response containing tenant details with HAL links",
      type: :object,
      properties: %{
        data: TenantFull,
        _links: QueryServiceExWeb.Schemas.Common.HALLinks
      },
      required: [:data]
    })
  end

  defmodule UsageStats do
    @moduledoc "Usage statistics"
    OpenApiSpex.schema(%{
      title: "UsageStats",
      description: "Usage statistics for a tenant",
      type: :object,
      properties: %{
        events: %Schema{
          type: :object,
          properties: %{
            used: %Schema{type: :integer},
            quota: %Schema{type: :integer},
            percentage: %Schema{type: :number, format: :float}
          }
        },
        queries: %Schema{
          type: :object,
          properties: %{
            used: %Schema{type: :integer},
            quota: %Schema{type: :integer},
            percentage: %Schema{type: :number, format: :float}
          }
        },
        reset_at: %Schema{type: :string, format: :"date-time"}
      }
    })
  end

  defmodule UsageResponse do
    @moduledoc "Usage statistics response"
    OpenApiSpex.schema(%{
      title: "UsageResponse",
      description: "Response containing usage statistics with HAL links",
      type: :object,
      properties: %{
        _links: QueryServiceExWeb.Schemas.Common.HALLinks,
        data: %Schema{
          type: :object,
          properties: %{
            tenant_id: %Schema{type: :string, format: :uuid},
            subscription_tier: %Schema{type: :string},
            subscription_status: %Schema{type: :string},
            events: %Schema{type: :object},
            queries: %Schema{type: :object},
            extraction_tokens: %Schema{
              type: :object,
              description: "Hosted Hound extraction LLM tokens used this billing period"
            },
            billing_period: %Schema{
              type: :object,
              properties: %{
                reset_at: %Schema{type: :string, format: :"date-time"}
              }
            }
          }
        }
      },
      required: [:data],
      example: %{
        data: %{
          tenant_id: "987fcdeb-51a2-43e1-b2c4-123456789000",
          subscription_tier: "growth",
          subscription_status: "active",
          events: %{used: 50_000, quota: 1_000_000, percentage: 5.0},
          queries: %{used: 5000, quota: 100_000, percentage: 5.0},
          extraction_tokens: %{used: 12_000, quota: 500_000, remaining: 488_000},
          billing_period: %{reset_at: "2026-03-01T00:00:00Z"}
        }
      }
    })
  end
end
