defmodule McpServerElixir.Protocol.EventHistoryResult do
  @moduledoc "The host-visible projection of a workspace timeline."
  alias McpServerElixir.Protocol.CustomerEvidenceSchema

  @entity %{type: "string", minLength: 1, maxLength: 128}
  @date %{type: "string", minLength: 20, maxLength: 40}
  @version %{type: "string", pattern: "^[A-Za-z0-9._-]{1,48}$"}
  @event %{
    id: %{type: "string", pattern: "^[0-9a-f-]{36}$"},
    entity: @entity,
    timestamp: @date,
    url: %{type: "string", maxLength: 2048}
  }

  def schema do
    choices = [
      object(Map.merge(@event, %{kind: %{enum: ["released"]}, version: @version})),
      object(
        Map.merge(@event, %{
          kind: %{enum: ["rolled_back"]},
          from_version: @version,
          to_version: @version
        })
      ),
      object(Map.put(@event, :kind, %{enum: ["recorded"]}))
    ]

    object(%{
      workspace: object(%{id: @entity, name: %{type: "string", minLength: 1, maxLength: 256}}),
      entity: @entity,
      from: @date,
      to: @date,
      coverage: %{enum: ["complete", "partial"]},
      events: %{type: "array", maxItems: 20, items: %{oneOf: choices}}
    })
  end

  def valid?(
        %{"entity" => entity, "workspace" => %{"id" => tenant}, "events" => rows} = data,
        entity,
        origin
      )
      when is_list(rows) and is_binary(origin) do
    CustomerEvidenceSchema.valid?(schema(), data) and
      Enum.all?(rows, fn row ->
        row["entity"] == entity and
          row["url"] ==
            origin <>
              "/event-history/workspaces/" <> URI.encode_www_form(tenant) <> "/events/" <> row["id"]
      end)
  end

  def valid?(_, _, _), do: false

  defp object(properties),
    do: %{
      type: "object",
      additionalProperties: false,
      properties: properties,
      required: properties |> Map.keys() |> Enum.map(&Atom.to_string/1)
    }
end
