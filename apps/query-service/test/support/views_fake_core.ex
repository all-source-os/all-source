defmodule QueryServiceEx.Views.FakeCore do
  @moduledoc """
  In-memory stand-in for Core's event feed, read models and admin view listing,
  with the same cursor, compare-and-set and reset rules, for view folding tests.
  """

  use Agent

  @empty_model %{revision: 0, checkpoint: nil, ready: false, rows: %{}}

  def start_link(opts \\ []) do
    initial = %{
      epoch: Keyword.get(opts, :epoch, 1),
      events: Keyword.get(opts, :events, []),
      models: Keyword.get(opts, :models, %{}),
      views: Keyword.get(opts, :views, []),
      page_size: nil,
      fail_feed: 0,
      conflicts: %{},
      commits: [],
      row_reads: 0
    }

    Agent.start_link(fn -> initial end, name: __MODULE__)
  end

  def update(fun), do: Agent.update(__MODULE__, fun)
  def get(fun), do: Agent.get(__MODULE__, fun)

  def model(name), do: get(&Map.get(&1.models, name, @empty_model))
  def commits, do: get(&Enum.reverse(&1.commits))

  def put_model(name, model) do
    update(&put_in(&1, [:models, name], Map.merge(@empty_model, model)))
  end

  def add_events(events), do: update(&%{&1 | events: &1.events ++ events})

  @doc "Make the next commit to `name` conflict; `on_conflict` rewrites the model first."
  def conflict_next(name, on_conflict \\ & &1) do
    update(&put_in(&1, [:conflicts, name], on_conflict))
  end

  def event_feed(_tenant, cursor, prefixes, limit) do
    Agent.get_and_update(__MODULE__, fn state ->
      cond do
        state.fail_feed > 0 ->
          {{:error, :core_unavailable}, %{state | fail_feed: state.fail_feed - 1}}

        is_map(cursor) and cursor["epoch"] != state.epoch ->
          {{:error, {:epoch_changed, state.epoch}}, state}

        true ->
          {{:ok, feed_page(state, cursor, prefixes, limit)}, state}
      end
    end)
  end

  defp feed_page(state, cursor, prefixes, limit) do
    start = if is_map(cursor), do: cursor["position"], else: 0
    size = state.page_size || limit
    head = length(state.events)

    candidates =
      state.events
      |> Enum.with_index(1)
      |> Enum.drop(start)
      |> Enum.filter(fn {event, _position} -> in_prefixes?(event, prefixes) end)
      |> Enum.take(size)

    stop =
      if length(candidates) == size and size > 0,
        do: candidates |> List.last() |> elem(1),
        else: head

    %{
      "tenant_id" => "fake",
      "cursor" => %{"epoch" => state.epoch, "position" => stop},
      "head" => %{"epoch" => state.epoch, "position" => head},
      "events" => Enum.map(candidates, &elem(&1, 0)),
      "positions" => Enum.map(candidates, &elem(&1, 1)),
      "caught_up" => stop >= head
    }
  end

  defp in_prefixes?(_event, []), do: true

  defp in_prefixes?(event, prefixes),
    do: Enum.any?(prefixes, &String.starts_with?(event["event_type"], &1))

  def read_model_page(_tenant, name, opts) do
    Agent.get_and_update(__MODULE__, fn state ->
      model = Map.get(state.models, name, @empty_model)
      prefix = Keyword.get(opts, :prefix, "")
      limit = Keyword.get(opts, :limit, 100)

      rows =
        model.rows
        |> Enum.filter(fn {key, _row} -> String.starts_with?(key, prefix) end)
        |> Enum.sort()
        |> Enum.take(limit)
        |> Map.new()

      page = %{
        "revision" => model.revision,
        "content_revision" => model.revision,
        "checkpoint" => model.checkpoint,
        "ready" => model.ready,
        "unchanged" => false,
        "rows" => rows,
        "next" => nil
      }

      row_read = prefix != "" and not String.starts_with?(name, "view_G")
      reads = if row_read, do: state.row_reads + 1, else: state.row_reads
      {{:ok, page}, %{state | row_reads: reads}}
    end)
  end

  def commit_read_model(_tenant, name, commit) do
    Agent.get_and_update(__MODULE__, fn state ->
      model = Map.get(state.models, name, @empty_model)

      case Map.pop(state.conflicts, name) do
        {nil, _conflicts} ->
          apply_commit(state, name, model, commit)

        {on_conflict, conflicts} ->
          moved = on_conflict.(%{model | revision: model.revision + 1})
          state = %{state | conflicts: conflicts, models: Map.put(state.models, name, moved)}
          {{:error, {:version_conflict, moved.revision}}, state}
      end
    end)
  end

  defp apply_commit(state, name, model, commit) do
    if commit.expected_revision != model.revision do
      {{:error, {:version_conflict, model.revision}}, state}
    else
      base = if commit.reset, do: %{}, else: model.rows

      rows =
        Enum.reduce(commit.changes, base, fn
          {key, nil}, acc -> Map.delete(acc, key)
          {key, row}, acc -> Map.put(acc, key, row)
        end)

      next = %{
        revision: model.revision + 1,
        checkpoint: commit.checkpoint,
        ready: commit.ready,
        rows: rows
      }

      state = %{
        state
        | models: Map.put(state.models, name, next),
          commits: [{name, commit} | state.commits]
      }

      {{:ok, next.revision}, state}
    end
  end

  def list_all_views do
    case get(& &1.views) do
      views when is_list(views) -> {:ok, views}
      reason -> {:error, reason}
    end
  end
end
