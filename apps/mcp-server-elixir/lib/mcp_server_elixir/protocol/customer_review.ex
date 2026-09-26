defmodule McpServerElixir.Protocol.CustomerReview do
  @moduledoc """
  Exclusive customer profile on the existing stdio server. Only context and
  syntax validation are currently wired. No fallback to general Core/admin tools,
  no preparation claim, and no human approval tool. Errors never echo inputs.
  """

  alias McpServerElixir.Infrastructure.CustomerReviewClient

  @protocol "2025-06-18"
  @version Mix.Project.config()[:version]
  @empty %{type: "object", properties: %{}, additionalProperties: false}
  @annotations %{
    readOnlyHint: true,
    destructiveHint: false,
    idempotentHint: true,
    openWorldHint: false
  }

  def handle_line(line) when is_binary(line) and byte_size(line) <= 65_536 do
    case Jason.decode(line) do
      {:ok, %{"jsonrpc" => "2.0", "method" => method} = request} when is_binary(method) ->
        dispatch(request)

      {:ok, _} ->
        error(nil, -32_600, "Invalid JSON-RPC request")

      {:error, _} ->
        error(nil, -32_700, "Invalid JSON")
    end
  rescue
    _ -> error(nil, -32_603, "Customer review unavailable")
  end

  def handle_line(_), do: error(nil, -32_600, "Request exceeds 64 KiB limit")

  def tools do
    [
      %{
        name: "allsource_review_context",
        description:
          "Check current connection eligibility. Does not retrieve events, prepare a review or approve an action.",
        inputSchema: @empty,
        outputSchema: context_output(),
        annotations: @annotations
      },
      %{
        name: "allsource_validate_review_proposal",
        description:
          "Validate a review proposal's syntax and curated projection target. Source references remain unresolved; nothing is persisted or approved.",
        inputSchema: proposal_schema(),
        outputSchema: validation_output(),
        annotations: @annotations
      }
    ]
  end

  # Notifications never produce a response, including unknown notifications.
  defp dispatch(request) when not is_map_key(request, "id"), do: nil

  defp dispatch(%{"id" => id}) when not (is_binary(id) or is_integer(id)),
    do: error(nil, -32_600, "Invalid request identifier")

  defp dispatch(%{"method" => "initialize", "id" => id, "params" => params}) when is_map(params) do
    result(id, %{
      protocolVersion: @protocol,
      capabilities: %{tools: %{}},
      serverInfo: %{name: "allsource-customer-review", version: @version},
      instructions:
        "AllSource owns review and human approval. These bindings currently check eligibility and syntax only; source authority and preparation remain unavailable."
    })
  end

  defp dispatch(%{"method" => "ping", "id" => id}), do: result(id, %{})
  defp dispatch(%{"method" => "tools/list", "id" => id}), do: result(id, %{tools: tools()})

  defp dispatch(%{"method" => "tools/call", "id" => id, "params" => params}) when is_map(params) do
    case {params["name"], Map.get(params, "arguments", %{})} do
      {"allsource_review_context", arguments} when is_map(arguments) and map_size(arguments) == 0 ->
        call(id, "context", %{})

      {"allsource_validate_review_proposal", %{"proposal" => proposal} = arguments}
      when is_map(proposal) and map_size(arguments) == 1 ->
        call(id, "validate", arguments)

      _ ->
        error(id, -32_602, "Unknown tool or invalid arguments")
    end
  end

  defp dispatch(%{"id" => id}),
    do: error(id, -32_601, "Method not available in customer review profile")

  defp call(id, operation, arguments) do
    case CustomerReviewClient.call(operation, arguments) do
      {:ok, data} ->
        result(id, %{
          content: [%{type: "text", text: Jason.encode!(data)}],
          structuredContent: data,
          isError: false
        })

      {:error, reason} ->
        result(id, %{content: [%{type: "text", text: message(reason)}], isError: true})
    end
  end

  defp message(:access_denied),
    do:
      "Access denied. Reconnect through AllSource; current membership, entitlement and grant scope are required."

  defp message(:invalid_proposal),
    do: "Invalid proposal. Use the published schema; source references remain unresolved."

  defp message(:rate_limited), do: "Rate limited. Wait before retrying."

  defp message(_),
    do:
      "Connection unavailable. Check the AllSource connection configuration; no work was prepared or approved."

  defp result(id, payload), do: %{jsonrpc: "2.0", id: id, result: payload}

  defp error(id, code, message),
    do: %{jsonrpc: "2.0", id: id, error: %{code: code, message: message}}

  defp proposal_schema do
    source = %{
      type: "object",
      additionalProperties: false,
      required: ["kind", "ref", "revision", "sha256"],
      properties: %{
        kind: %{
          type: "string",
          enum: ["event_range", "restart_evidence", "run_evidence", "replay_analysis"]
        },
        ref: %{type: "string", minLength: 16, maxLength: 128, pattern: "^[A-Za-z0-9_-]+$"},
        revision: %{type: "integer", minimum: 1, maximum: 9_007_199_254_740_991},
        sha256: %{type: "string", pattern: "^[0-9a-f]{64}$"}
      }
    }

    %{
      type: "object",
      additionalProperties: false,
      required: ["proposal"],
      properties: %{
        proposal: %{
          type: "object",
          additionalProperties: false,
          required: ["schema_version", "kind", "projection_name", "sources"],
          properties: %{
            schema_version: %{type: "integer", const: 1},
            kind: %{type: "string", enum: ["event_timeline", "run_comparison", "replay_plan"]},
            projection_name: %{type: ["string", "null"], maxLength: 64},
            sources: %{type: "array", maxItems: 32, items: source}
          }
        }
      }
    }
  end

  defp context_output do
    %{
      type: "object",
      additionalProperties: false,
      required: [
        "state",
        "mcp_scope",
        "operations",
        "source_access",
        "human_approval",
        "preparation_available"
      ],
      properties: %{
        state: %{const: "eligibility_verified"},
        mcp_scope: %{
          type: "string",
          enum: ["read", "read+write", "read+write+dedicated", "dedicated"]
        },
        operations: %{
          type: "array",
          maxItems: 2,
          uniqueItems: true,
          items: %{enum: ["read_context", "validate_proposal"]}
        },
        source_access: %{const: "unresolved"},
        human_approval: %{const: "required_in_product"},
        preparation_available: %{const: false}
      }
    }
  end

  defp validation_output do
    %{
      type: "object",
      additionalProperties: false,
      required: ["state", "fingerprint", "unknowns", "persisted", "approved"],
      properties: %{
        state: %{const: "valid_unresolved"},
        fingerprint: %{type: "string", pattern: "^[0-9a-f]{64}$"},
        unknowns: %{
          type: "array",
          maxItems: 2,
          uniqueItems: true,
          items: %{
            enum: [
              "missing_event_sources",
              "missing_comparison_source",
              "missing_replay_analysis",
              "unresolved_source_authority"
            ]
          }
        },
        persisted: %{const: false},
        approved: %{const: false}
      }
    }
  end
end
