defmodule McpServerElixir.Protocol.CustomerReplaySchema do
  @moduledoc "Closed replay preparation and result schemas; no agent approval or execution tool."
  @digest %{type: "string", pattern: "^[0-9a-f]{64}$"}
  @id %{type: "string", minLength: 16, maxLength: 128, pattern: "^[A-Za-z0-9_-]+$"}
  @uuid %{type: "string", pattern: "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"}
  @count %{type: "integer", minimum: 0, maximum: 9_007_199_254_740_991}
  @version %{type: "integer", minimum: 1, maximum: 1_000}
  @time %{type: "string", maxLength: 40}
  @target %{type: "string", maxLength: 64}

  def proposal do
    object(%{
      schema_version: %{enum: [1]},
      kind: %{enum: ["replay_plan"]},
      projection_name: @target,
      sources: %{
        type: "array",
        minItems: 1,
        maxItems: 1,
        items:
          object(%{
            kind: %{enum: ["replay_analysis"]},
            ref: @id,
            revision: %{enum: [1]},
            sha256: @digest
          })
      }
    })
  end

  def read_input(operation),
    do: object(%{id: @id, version: @version, digest: @digest, request_id: operation})

  def output do
    reference = %{
      schema_version: %{enum: [1]},
      view_schema: %{enum: ["rebuild-plan-v1"]},
      human_approval: %{enum: ["required_in_product"]},
      id: @id,
      version: @version,
      digest: @digest,
      state: %{enum: ~w(pending approved rejected)},
      expires_at: @count,
      replay_operation_id: @uuid
    }

    retired = object(Map.put(reference, :effective_state, %{enum: ["expired"]}))

    full =
      object(
        Map.merge(reference, %{
          effective_state: %{enum: ~w(pending approved rejected superseded expired)},
          proposal: proposal(),
          evidence: evidence(),
          action: action(),
          decision: nullable(decision()),
          replay: nullable(replay()),
          approved_version: nullable(@version),
          approved_digest: nullable(@digest)
        })
      )

    %{oneOf: [retired, full]}
  end

  defp action,
    do:
      object(%{
        operation: %{enum: ["start_tenant_projection_rebuild"]},
        projection_name: @target,
        history: %{enum: ["retained_at_dispatch"]},
        live_catchup: %{enum: [true]}
      })

  defp decision,
    do:
      object(%{
        id: @uuid,
        actor: %{type: "string", maxLength: 256},
        role: %{enum: ["admin"]},
        version: @version,
        digest: @digest,
        at: @count,
        expires_at: @count,
        operation: %{enum: ["start_tenant_projection_rebuild"]},
        replay_operation_id: @uuid
      })

  defp evidence,
    do:
      object(%{
        sample_sha256: @digest,
        catalog_sha256: @digest,
        analysis:
          object(%{
            analysis_scope: %{enum: [nil, "sample", "full"]},
            analyzed_at: @time,
            projection_name: @target,
            projection_status: %{enum: [nil, "ready", "building"]},
            sampled_events: %{type: "integer", minimum: 0, maximum: 1_000},
            sampled_entity_count: %{type: "integer", minimum: 0, maximum: 1_000},
            current_entity_count: nullable(@count),
            reported_total_events: nullable(@count),
            unknowns: %{
              enum: [
                ~w(total_count_provenance authoritative_order restart_proof run_comparison archive_completeness)
              ]
            }
          })
      })

  defp replay do
    %{
      oneOf: [
        object(%{status: %{enum: ["not_started"]}}),
        object(%{
          operation_id: @uuid,
          replay_id: @id,
          projection_name: @target,
          request_sha256: @digest,
          cutoff: @time,
          status: %{enum: ~w(unknown running completed failed cancelled)},
          completed_at: nullable(@time),
          processed_events: nullable(@count)
        })
      ]
    }
  end

  defp nullable(schema), do: %{anyOf: [schema, %{type: "null"}]}

  defp object(properties),
    do: %{
      type: "object",
      properties: properties,
      required: Enum.map(Map.keys(properties), &to_string/1),
      additionalProperties: false
    }
end
