defmodule McpServerElixir.Protocol.CustomerEvidenceSchema do
  @moduledoc "Closed schemas for product-selected comparison evidence. No approval operation."
  @digest %{type: "string", pattern: "^[0-9a-f]{64}$"}
  @id %{type: "string", minLength: 16, maxLength: 128, pattern: "^[A-Za-z0-9_-]+$"}
  @integer %{type: "integer", minimum: 1, maximum: 9_007_199_254_740_991}
  @text %{type: "string", maxLength: 128}
  @operation %{
    type: "string",
    minLength: 38,
    maxLength: 53,
    pattern: "^[1-9][0-9]{0,15}:[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$",
    description:
      "UTC seconds plus UUID, created once for this operation. Keep the exact key and input on interrupted retries; valid for one hour."
  }

  def tools do
    [
      tool(
        "allsource_prepare_review",
        "prepare",
        "Prepare a pending comparison of exactly two product-selected run pins. Costs two admitted queries; exact retries use the same key. Does not approve or execute.",
        object(%{
          proposal: proposal(),
          expected_revision: %{enum: [0]},
          idempotency_key: @operation
        })
      ),
      tool(
        "allsource_get_review",
        "review",
        "Read a pending review and current pinned evidence. Costs two queries for each new request key; reuse the key on retry. Expired, changed or unavailable sources return status only.",
        read_input()
      ),
      tool(
        "allsource_get_review_result",
        "result",
        "Check result status for the exact review version. Currently returns pending with result_available=false; no human approval or execution is implemented. A new live check costs two queries.",
        read_input()
      )
    ]
  end

  defp tool(name, operation, description, input) do
    %{
      name: name,
      description: description,
      inputSchema: input,
      outputSchema: output(operation),
      annotations: %{
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false
      }
    }
  end

  def operation("allsource_prepare_review"), do: "prepare"
  def operation("allsource_get_review"), do: "review"
  def operation("allsource_get_review_result"), do: "result"

  def valid?(schema, value) do
    # The installed validator supports Draft 4. Normalize constant constraints to enums.
    schema = schema |> Jason.encode!() |> Jason.decode!() |> draft_four()
    ExJsonSchema.Validator.valid?(ExJsonSchema.Schema.resolve(schema), value)
  rescue
    _ -> false
  end

  defp draft_four(value) when is_map(value) do
    value =
      if Map.has_key?(value, "const"),
        do: value |> Map.put("enum", [value["const"]]) |> Map.delete("const"),
        else: value

    Map.new(value, fn {key, item} -> {key, draft_four(item)} end)
  end

  defp draft_four(value) when is_list(value), do: Enum.map(value, &draft_four/1)
  defp draft_four(value), do: value

  defp read_input, do: object(%{id: @id, version: %{enum: [1]}, request_id: @operation})

  defp proposal do
    source =
      object(%{kind: %{enum: ["run_evidence"]}, ref: @id, revision: @integer, sha256: @digest})

    object(%{
      schema_version: %{enum: [1]},
      kind: %{enum: ["run_comparison"]},
      projection_name: %{enum: [nil]},
      sources: %{type: "array", minItems: 2, maxItems: 2, items: source}
    })
  end

  defp output(operation) do
    properties = %{
      schema_version: %{enum: [1]},
      id: @id,
      version: %{enum: [1]},
      digest: @digest,
      state: %{enum: ["pending"]},
      expires_at: @integer,
      view_schema: %{enum: ["run-comparison-v1"]},
      persisted: %{enum: [true]},
      approved: %{enum: [false]},
      execution: %{enum: ["none"]},
      human_approval: %{enum: ["required_in_product"]}
    }

    pending =
      case operation do
        "prepare" ->
          Map.put(properties, :unknowns, array(@text))

        "review" ->
          Map.merge(properties, %{
            unknowns: array(@text),
            proposal: proposal(),
            evidence: evidence()
          })

        "result" ->
          Map.put(properties, :result_available, %{enum: [false]})
      end

    retired = Map.put(properties, :state, %{enum: ["expired", "superseded", "unavailable"]})

    if operation == "prepare",
      do: object(pending),
      else: %{type: "object", oneOf: [object(pending), object(retired)]}
  end

  defp evidence do
    pin = object(%{run_id: @text, revision: @integer, digest: @digest})
    optional_digest = %{anyOf: [@digest, %{type: "null"}]}

    attempt =
      object(%{
        state: @text,
        test_outcome: %{type: ["string", "null"], maxLength: 128},
        test_sha256: optional_digest
      })

    recorded = %{
      type: "object",
      additionalProperties: false,
      required: ["kind"],
      properties: %{
        kind: @text,
        evidence_sha256: optional_digest,
        outcome: %{type: ["string", "null"], maxLength: 128}
      }
    }

    signature =
      object(%{
        state: @text,
        evidence_sha256: optional_digest,
        attempts: array(attempt),
        evidence: array(recorded)
      })

    side = %{anyOf: [signature, %{type: "null"}]}
    difference = object(%{change_number: @integer, baseline: side, candidate: side})

    object(%{
      state: %{enum: ["same_recorded_evidence", "inconclusive", "divergent"]},
      baseline: pin,
      candidate: pin,
      descriptor_changes: array(%{enum: ~w(agent_sha256 model_sha256 prompt_sha256)}),
      first_divergence: %{anyOf: [difference, %{type: "null"}]},
      differences: array(difference),
      unknowns: array(@text),
      execution: %{enum: ["none"]},
      approval_authority: %{enum: ["not_established"]}
    })
  end

  defp object(properties),
    do: %{
      type: "object",
      properties: properties,
      required: Enum.map(Map.keys(properties), &to_string/1),
      additionalProperties: false
    }

  defp array(items), do: %{type: "array", items: items}
end
