defmodule QueryServiceEx.Projections.FoldEngineTest do
  @moduledoc """
  Unit tests for fold, filter, and paginate logic over a projection module.
  Tests the projection pipeline in isolation
  without needing Core or auth.
  """
  use ExUnit.Case, async: true

  defmodule TestProjection do
    @behaviour QueryServiceEx.Projections.Behaviour

    @impl true
    def entity_type, do: "widget"

    @impl true
    def initial_state, do: %{}

    @impl true
    def apply_event(state, %{"event_type" => "widget.created"} = event) do
      Map.merge(state, %{
        "id" => event["entity_id"],
        "name" => get_in(event, ["data", "name"]),
        "is_deleted" => false,
        "created_at" => event["timestamp"],
        "updated_at" => event["timestamp"]
      })
    end

    def apply_event(state, %{"event_type" => "widget.updated"} = event) do
      state
      |> Map.merge(event["data"] || %{})
      |> Map.put("updated_at", event["timestamp"])
    end

    def apply_event(state, %{"event_type" => "widget.deleted"} = event) do
      Map.merge(state, %{"is_deleted" => true, "updated_at" => event["timestamp"]})
    end

    def apply_event(state, _event), do: state

    @impl true
    def filterable_fields, do: ["is_deleted", "name"]
  end

  describe "fold_events" do
    test "groups by entity_id and reduces" do
      events = [
        %{
          "entity_id" => "w1",
          "event_type" => "widget.created",
          "timestamp" => "2024-01-01T00:00:00Z",
          "data" => %{"name" => "Alpha"}
        },
        %{
          "entity_id" => "w2",
          "event_type" => "widget.created",
          "timestamp" => "2024-01-02T00:00:00Z",
          "data" => %{"name" => "Beta"}
        }
      ]

      result = fold_events(events, TestProjection)

      assert length(result) == 2
      names = Enum.map(result, & &1["name"]) |> Enum.sort()
      assert names == ["Alpha", "Beta"]
    end

    test "applies events in timestamp order" do
      events = [
        %{
          "entity_id" => "w1",
          "event_type" => "widget.updated",
          "timestamp" => "2024-01-03T00:00:00Z",
          "data" => %{"name" => "Renamed"}
        },
        %{
          "entity_id" => "w1",
          "event_type" => "widget.created",
          "timestamp" => "2024-01-01T00:00:00Z",
          "data" => %{"name" => "Original"}
        }
      ]

      [state] = fold_events(events, TestProjection)

      # Created first (sorted by timestamp), then updated → name is "Renamed"
      assert state["name"] == "Renamed"
      assert state["updated_at"] == "2024-01-03T00:00:00Z"
    end

    test "handles delete events" do
      events = [
        %{
          "entity_id" => "w1",
          "event_type" => "widget.created",
          "timestamp" => "2024-01-01T00:00:00Z",
          "data" => %{"name" => "Alpha"}
        },
        %{
          "entity_id" => "w1",
          "event_type" => "widget.deleted",
          "timestamp" => "2024-01-02T00:00:00Z",
          "data" => %{}
        }
      ]

      [state] = fold_events(events, TestProjection)
      assert state["is_deleted"] == true
    end

    test "returns empty list for no events" do
      assert fold_events([], TestProjection) == []
    end
  end

  describe "apply_filters" do
    test "filters on allowed fields" do
      items = [
        %{"name" => "Alpha", "is_deleted" => false},
        %{"name" => "Beta", "is_deleted" => true},
        %{"name" => "Gamma", "is_deleted" => false}
      ]

      result = apply_filters(items, %{"is_deleted" => false}, TestProjection)
      assert length(result) == 2
      assert Enum.all?(result, &(&1["is_deleted"] == false))
    end

    test "ignores filters on non-filterable fields" do
      items = [
        %{"name" => "Alpha", "secret" => "hidden"}
      ]

      result = apply_filters(items, %{"secret" => "hidden"}, TestProjection)
      # "secret" not in filterable_fields, so filter is ignored → all items returned
      assert length(result) == 1
    end

    test "returns all items when filters are empty" do
      items = [%{"name" => "Alpha"}, %{"name" => "Beta"}]
      result = apply_filters(items, %{}, TestProjection)
      assert length(result) == 2
    end
  end

  describe "paginate" do
    test "paginates with default page 1" do
      items = Enum.map(1..10, &%{"id" => &1})
      {page, result} = paginate(items, %{"page" => 1, "page_size" => 3})
      assert length(result) == 3
      assert page.number == 1
      assert page.size == 3
    end

    test "paginates page 2" do
      items = Enum.map(1..10, &%{"id" => &1})
      {_page, result} = paginate(items, %{"page" => 2, "page_size" => 3})
      assert length(result) == 3
      assert List.first(result)["id"] == 4
    end

    test "returns empty when page is beyond data" do
      items = [%{"id" => 1}]
      {_page, result} = paginate(items, %{"page" => 5, "page_size" => 10})
      assert result == []
    end

    test "sorts by field descending" do
      items = [%{"name" => "Alpha"}, %{"name" => "Gamma"}, %{"name" => "Beta"}]

      {_page, result} =
        paginate(items, %{
          "page" => 1,
          "page_size" => 10,
          "sort_by" => "name",
          "sort_order" => "desc"
        })

      assert Enum.map(result, & &1["name"]) == ["Gamma", "Beta", "Alpha"]
    end

    test "sorts ascending by default" do
      items = [%{"name" => "Gamma"}, %{"name" => "Alpha"}, %{"name" => "Beta"}]

      {_page, result} =
        paginate(items, %{
          "page" => 1,
          "page_size" => 10,
          "sort_by" => "name"
        })

      assert Enum.map(result, & &1["name"]) == ["Alpha", "Beta", "Gamma"]
    end
  end

  # Helpers that mirror the controller's private functions for direct unit testing

  defp fold_events(events, projection_mod) do
    events
    |> Enum.group_by(& &1["entity_id"])
    |> Enum.map(fn {_entity_id, entity_events} ->
      entity_events
      |> Enum.sort_by(& &1["timestamp"])
      |> Enum.reduce(projection_mod.initial_state(), fn event, state ->
        projection_mod.apply_event(state, event)
      end)
    end)
  end

  defp apply_filters(projected, filters, projection_mod) when map_size(filters) == 0 do
    _ = projection_mod
    projected
  end

  defp apply_filters(projected, filters, projection_mod) do
    allowed = MapSet.new(projection_mod.filterable_fields())

    valid_filters =
      filters
      |> Enum.filter(fn {key, _val} -> MapSet.member?(allowed, key) end)
      |> Map.new()

    Enum.filter(projected, fn state ->
      Enum.all?(valid_filters, fn {key, value} ->
        Map.get(state, key) == value
      end)
    end)
  end

  defp paginate(items, params) do
    page_number = Map.get(params, "page", 1)
    page_size = Map.get(params, "page_size", 50)

    result =
      items
      |> maybe_sort(params["sort_by"], params["sort_order"])
      |> Enum.drop((page_number - 1) * page_size)
      |> Enum.take(page_size)

    {%{number: page_number, size: page_size}, result}
  end

  defp maybe_sort(items, nil, _order), do: items

  defp maybe_sort(items, sort_by, sort_order) do
    sorter =
      case sort_order do
        "desc" -> &>=/2
        _ -> &<=/2
      end

    Enum.sort_by(items, &Map.get(&1, sort_by), sorter)
  end
end
