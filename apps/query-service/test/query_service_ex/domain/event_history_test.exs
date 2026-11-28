defmodule QueryServiceEx.Domain.EventHistoryTest do
  use ExUnit.Case, async: true
  alias QueryServiceEx.Domain.EventHistory, as: H
  @now ~U[2026-11-26 16:00:00Z]
  @space %{"id" => "engineering", "name" => "Engineering"}
  @origin "https://example.test"
  @first "2026-11-25T16:00:00Z"
  @last "2026-11-26T16:00:00Z"
  @filter %{"entity_id" => "checkout", "since" => @first, "until" => @last}
  @base %{
    "protocol" => "retained-history-v1",
    "tenant_id" => "engineering",
    "since" => @first,
    "until" => @last,
    "entity_id" => "checkout",
    "event_id" => nil,
    "read_point" => @last,
    "coverage" => "complete",
    "truncated" => false,
    "events" => []
  }
  @row %{
    "id" => "00000000-0000-4000-8000-000000000001",
    "tenant_id" => "engineering",
    "entity_id" => "checkout",
    "timestamp" => "2026-11-26T14:00:00.123456789Z",
    "event_type" => "deployment.released.v1",
    "payload" => %{"version" => "2.4", "extra" => "secret"}
  }

  test "bare question selects exact preceding UTC day; explicit windows normalize offsets without losing nanoseconds" do
    assert {:ok, @filter} = H.select(%{"entity" => "checkout"}, @now)

    for args <- [
          %{"tenant_id" => "other"},
          %{"from" => @first},
          %{"from" => @last, "to" => @first},
          %{"from" => "2026-10-01T00:00:00Z", "to" => @last},
          %{"from" => @first, "to" => "2026-11-26T16:00:00.000000001Z"}
        ] do
      assert {:error, :invalid_request} = H.select(Map.put(args, "entity", "checkout"), @now)
    end

    for entity <- [".", "..", "a/b", "a\\b", "a\nb", " a", String.duplicate("a", 129), nil],
        do: assert({:error, :invalid_request} == H.select(%{"entity" => entity}, @now))

    assert {:ok, %{"since" => "2026-11-26T12:00:00.000000001Z"}} =
             H.select(
               %{
                 "entity" => "checkout",
                 "from" => "2026-11-26T13:00:00.000000001+01:00",
                 "to" => @last
               },
               @now
             )
  end

  test "strict row provenance, bound windows and allowlisted payload" do
    assert {:ok, %{"events" => [e]}} = render([@row])

    assert e == %{
             "id" => @row["id"],
             "entity" => "checkout",
             "kind" => "released",
             "version" => "2.4",
             "timestamp" => @row["timestamp"],
             "url" => @origin <> "/event-history/workspaces/engineering/events/" <> @row["id"]
           }

    for {field, value} <- [
          {"id", "bad"},
          {"tenant_id", "other"},
          {"entity_id", "other"},
          {"timestamp", @last}
        ] do
      assert {:error, :source_unavailable} = render([Map.put(@row, field, value)])
    end

    for row <- [
          put_in(@row["payload"]["version"], "a b"),
          Map.put(@row, "event_type", "do-anything")
        ] do
      assert {:ok, %{"events" => [item]}} = render([row])
      assert item["kind"] == "recorded" and not Map.has_key?(item, "version")
    end

    assert {:error, :source_unavailable} =
             render([], %{"since" => "2026-11-25T16:00:00.000000001Z"})

    assert {:error, :source_unavailable} = render([], %{"tenant_id" => "other"})
  end

  test "coverage and source ordering survive same-microsecond events and truncation" do
    assert {:ok, %{"coverage" => "complete", "events" => []}} = render([])

    assert {:ok, %{"coverage" => "partial", "events" => []}} =
             render([], %{"coverage" => "partial"})

    assert {:error, :source_unavailable} = render([], %{"read_point" => nil})

    rows =
      for i <- 21..1//-1,
          do:
            Map.put(
              @row,
              "id",
              "00000000-0000-4000-8000-" <> String.pad_leading(to_string(i), 12, "0")
            )

    assert {:ok, %{"coverage" => "partial", "events" => output}} = render(rows)
    assert length(output) == 20
    assert {:error, :source_unavailable} = render(Enum.reverse(rows))
    newer = Map.put(@row, "timestamp", "2026-11-26T14:00:00.123456790Z")
    older = Map.put(@row, "id", "00000000-0000-4000-8000-000000000099")
    assert {:ok, _} = render([newer, older])
    assert {:error, :source_unavailable} = render([older, newer])
  end

  test "exact source identity required before evidence projection" do
    page =
      Map.merge(@base, %{
        "entity_id" => nil,
        "since" => nil,
        "until" => nil,
        "event_id" => @row["id"],
        "events" => [@row]
      })

    assert {:ok, %{"event" => %{"id" => id}}} = H.evidence(page, @space, @row["id"], @origin)
    assert id == @row["id"]
    empty = Map.put(page, "events", [])
    assert {:error, :not_found} = H.evidence(empty, @space, id, @origin)

    assert {:error, :source_unavailable} =
             H.evidence(%{empty | "coverage" => "partial"}, @space, id, @origin)

    assert {:error, :source_unavailable} =
             H.evidence(page, @space, "00000000-0000-4000-8000-000000000099", @origin)
  end

  defp render(rows, changes \\ %{}),
    do:
      H.timeline(@base |> Map.put("events", rows) |> Map.merge(changes), @space, @filter, @origin)
end
