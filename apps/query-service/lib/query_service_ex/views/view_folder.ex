defmodule QueryServiceEx.Views.ViewFolder do
  @moduledoc """
  Folds one tenant's view from Core's event feed into the Core read model
  `view_<name>` (latest-state contract, section 4; ADR-021).

  Each pass tails the feed from the model's stored checkpoint, applies
  `QueryServiceEx.Views.Reducer` to every event and commits the changed rows
  with compare-and-set on the model revision, in batches of at most 500 changes
  and 240 KiB. Rows are read from Core only when the bounded in-memory cache
  does not hold them.

  A rebuild starts at position 0 of the current feed epoch and sends
  `reset: true` with its first commit, so the old rows keep serving until then.
  It happens when Core reports a new feed epoch, and when the generation this
  folder runs differs from the one recorded in the marker model
  `view_G<name>`. The uppercase `G` cannot occur in a view name, so the marker
  never collides with a view's rows and is reserved like every `view_*` model.
  """

  use GenServer, restart: :transient

  require Logger

  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient
  alias QueryServiceEx.Views.Reducer

  @pubsub QueryServiceEx.PubSub
  @feed_limit 200
  @max_changes 500
  @max_commit_bytes 240 * 1024 - 8 * 1024
  @max_cached_rows 5_000
  @idle_ms 5_000
  @base_backoff_ms 500
  @max_backoff_ms 30_000
  @max_steps_per_sync 1_000
  @conflict_retries 3

  ## Client API

  @doc """
  Options: `:tenant_id`, `:view`, `:generation`, `:definition` (required);
  `:core` (module with the `RustCoreClient` feed and read-model functions),
  `:idle_ms`, `:subscribe` and `:auto` (false disables timers; drive with `sync/1`).
  """
  def start_link(opts) do
    tenant_id = Keyword.fetch!(opts, :tenant_id)
    view = Keyword.fetch!(opts, :view)
    GenServer.start_link(__MODULE__, opts, name: via(tenant_id, view))
  end

  @doc "Registry name of the folder for `{tenant_id, view}`."
  def via(tenant_id, view) do
    {:via, Registry, {QueryServiceEx.ProjectionRegistry, {:view_folder, tenant_id, view}}}
  end

  @doc "Fold until caught up, then return the folder's status."
  def sync(server, timeout \\ 30_000), do: GenServer.call(server, :sync, timeout)

  @doc "Current revision, checkpoint, readiness and generation."
  def status(server), do: GenServer.call(server, :status)

  ## Server Callbacks

  @impl true
  def init(opts) do
    tenant_id = Keyword.fetch!(opts, :tenant_id)
    view = Keyword.fetch!(opts, :view)
    definition = Keyword.fetch!(opts, :definition)

    state = %{
      tenant_id: tenant_id,
      view: view,
      generation: Keyword.fetch!(opts, :generation),
      definition: definition,
      prefixes: List.wrap(get_in(definition, ["source", "prefixes"])),
      model: "view_" <> view,
      marker: "view_G" <> view,
      core: Keyword.get(opts, :core, RustCoreClient),
      idle_ms: Keyword.get(opts, :idle_ms, @idle_ms),
      auto: Keyword.get(opts, :auto, true),
      loaded: false,
      revision: 0,
      marker_revision: 0,
      cursor: nil,
      committed: nil,
      reset: false,
      ready: false,
      cache: %{},
      timer: nil,
      idle: false,
      backoff_ms: 0
    }

    if Keyword.get(opts, :subscribe, true) do
      Phoenix.PubSub.subscribe(@pubsub, "events:#{tenant_id}:all")
    end

    {:ok, schedule(state, 0)}
  end

  @impl true
  def handle_call(:sync, _from, state) do
    {reply, state} = sync_loop(state, 0)
    {:reply, reply, state}
  end

  def handle_call(:status, _from, state), do: {:reply, public_status(state), state}

  @impl true
  def handle_info(:tick, state) do
    state = %{state | timer: nil}

    case safe_step(state) do
      {:caught_up, state} ->
        {:noreply, schedule(%{state | backoff_ms: 0, idle: true}, state.idle_ms)}

      {:more, state} ->
        {:noreply, schedule(%{state | backoff_ms: 0, idle: false}, 0)}

      {{:error, reason}, state} ->
        backoff = next_backoff(state.backoff_ms)

        Logger.warning(
          "[ViewFolder] #{state.view} fold failed, retrying in #{backoff}ms: #{inspect(reason)}",
          tenant_id: state.tenant_id
        )

        emit(state, :errors, %{count: 1}, %{})
        {:noreply, schedule(%{state | backoff_ms: backoff, idle: false}, backoff)}
    end
  end

  def handle_info({:new_event, _event}, %{idle: true, timer: timer} = state) do
    if timer, do: Process.cancel_timer(timer)
    {:noreply, schedule(%{state | timer: nil, idle: false}, 0)}
  end

  def handle_info(_message, state), do: {:noreply, state}

  ## Loop

  defp sync_loop(state, steps) when steps >= @max_steps_per_sync,
    do: {{:error, :not_caught_up}, state}

  defp sync_loop(state, steps) do
    case safe_step(state) do
      {:caught_up, state} -> {{:ok, public_status(state)}, state}
      {:more, state} -> sync_loop(state, steps + 1)
      {{:error, reason}, state} -> {{:error, reason}, state}
    end
  end

  defp safe_step(state) do
    step(state)
  rescue
    error -> {{:error, error}, state}
  end

  defp step(%{loaded: false} = state) do
    case load(state) do
      {:ok, state} -> step(state)
      {:error, reason} -> {{:error, reason}, state}
    end
  end

  defp step(state) do
    case state.core.event_feed(state.tenant_id, state.cursor, state.prefixes, @feed_limit) do
      {:ok, page} -> fold_page(state, page)
      {:error, {:epoch_changed, _epoch}} -> {:more, rebuild(state)}
      {:error, reason} -> {{:error, reason}, state}
    end
  end

  defp load(state) do
    with {:ok, model} <- read_page(state, state.model, limit: 1),
         {:ok, marker} <- read_page(state, state.marker, prefix: "generation", limit: 1) do
      checkpoint = model["checkpoint"]
      resume = folded_generation(marker) == state.generation and is_map(checkpoint)

      state = %{
        state
        | loaded: true,
          revision: model["revision"] || 0,
          marker_revision: marker["revision"] || 0,
          cursor: if(resume, do: checkpoint),
          committed: checkpoint,
          ready: resume and model["ready"] == true,
          cache: %{}
      }

      {:ok, if(resume, do: state, else: rebuild(state))}
    end
  end

  defp folded_generation(%{"rows" => %{"generation" => %{"generation" => generation}}}),
    do: generation

  defp folded_generation(_marker), do: nil

  defp rebuild(state) do
    emit(state, :rebuilds, %{count: 1}, %{})
    %{state | cursor: nil, reset: true, ready: false, cache: %{}}
  end

  ## Folding one feed page

  defp fold_page(state, %{"cursor" => %{"epoch" => epoch} = page_cursor} = page) do
    events = List.wrap(page["events"])
    positions = List.wrap(page["positions"])
    caught_up = page["caught_up"] == true

    events
    |> Enum.zip(positions)
    |> Enum.reduce_while({:ok, state, new_batch()}, fn {event, position}, {:ok, state, batch} ->
      checkpoint = %{"epoch" => epoch, "position" => position}

      case fold_event(state, batch, event, checkpoint) do
        {:ok, state, batch} -> {:cont, {:ok, state, batch}}
        other -> {:halt, other}
      end
    end)
    |> finish_page(page_cursor, caught_up)
  end

  defp fold_page(state, _page), do: {{:error, :invalid_feed_page}, state}

  defp finish_page({:ok, state, batch}, page_cursor, caught_up) do
    ready = state.ready or caught_up
    needs_commit = map_size(batch.changes) > 0 or state.reset or (ready and not state.ready)

    result =
      if needs_commit,
        do: commit(state, batch, page_cursor, ready),
        else: {:ok, %{state | cache: merge_cache(state.cache, batch.reads, %{})}}

    outcome = if caught_up, do: :caught_up, else: :more

    case result do
      {:ok, state} -> {outcome, %{state | cursor: page_cursor}}
      {:resync, state} -> {:more, state}
      {:error, reason, state} -> {{:error, reason}, state}
    end
  end

  defp finish_page({:resync, state}, _page_cursor, _caught_up), do: {:more, state}

  defp finish_page({:error, reason, state}, _page_cursor, _caught_up),
    do: {{:error, reason}, state}

  defp fold_event(state, batch, event, checkpoint) do
    with {:ok, key} <- Reducer.key(state.definition, event),
         {:ok, row, batch} <- current_row(state, batch, key) do
      {result, stats} = Reducer.fold(state.definition, row, event)

      if stats.rejected_transition > 0 do
        emit(state, :rejected_transition, %{count: stats.rejected_transition}, %{})
      end

      add_change(state, batch, result, checkpoint)
    else
      {:skip, reason} ->
        emit(state, :skipped, %{count: 1}, %{reason: reason})
        {:ok, state, batch}

      {:error, reason} ->
        {:error, reason, state}
    end
  end

  defp add_change(state, batch, {:skip, reason}, _checkpoint) do
    emit(state, :skipped, %{count: 1}, %{reason: reason})
    {:ok, state, batch}
  end

  defp add_change(state, batch, result, checkpoint) do
    {key, row} =
      case result do
        {:upsert, key, row} -> {key, row}
        {:delete, key} -> {key, nil}
      end

    size = byte_size(key) + byte_size(Jason.encode!(row)) + 8

    case flush_if_full(state, batch, key, size) do
      {:ok, state, batch} -> {:ok, state, put_change(batch, key, row, size, checkpoint)}
      other -> other
    end
  end

  defp flush_if_full(state, batch, key, size) do
    new_key = if Map.has_key?(batch.changes, key), do: 0, else: 1
    bytes = batch.bytes - Map.get(batch.sizes, key, 0) + size
    full = map_size(batch.changes) + new_key > @max_changes or bytes > @max_commit_bytes

    if full and map_size(batch.changes) > 0 do
      case commit(state, batch, batch.checkpoint, state.ready) do
        {:ok, state} -> {:ok, state, new_batch()}
        other -> other
      end
    else
      {:ok, state, batch}
    end
  end

  defp new_batch, do: %{changes: %{}, sizes: %{}, bytes: 0, reads: %{}, checkpoint: nil}

  defp put_change(batch, key, row, size, checkpoint) do
    %{
      batch
      | changes: Map.put(batch.changes, key, row),
        sizes: Map.put(batch.sizes, key, size),
        bytes: batch.bytes - Map.get(batch.sizes, key, 0) + size,
        checkpoint: checkpoint
    }
  end

  defp current_row(state, batch, key) do
    cond do
      Map.has_key?(batch.changes, key) -> {:ok, batch.changes[key], batch}
      Map.has_key?(batch.reads, key) -> {:ok, batch.reads[key], batch}
      Map.has_key?(state.cache, key) -> {:ok, state.cache[key], batch}
      state.reset -> {:ok, nil, batch}
      true -> read_row(state, batch, key)
    end
  end

  defp read_row(state, batch, key) do
    case read_page(state, state.model, prefix: key, limit: 1) do
      {:ok, page} ->
        row = get_in(page, ["rows", key])
        {:ok, row, %{batch | reads: Map.put(batch.reads, key, row)}}

      {:error, reason} ->
        {:error, reason}
    end
  end

  ## Committing

  defp commit(state, batch, checkpoint, ready, attempt \\ 0) do
    body = %{
      expected_revision: state.revision,
      checkpoint: checkpoint,
      ready: ready,
      reset: state.reset,
      changes: batch.changes
    }

    case state.core.commit_read_model(state.tenant_id, state.model, body) do
      {:ok, revision} ->
        emit(state, :commits, %{count: 1, changes: map_size(batch.changes)}, %{})

        state = %{
          state
          | revision: revision,
            cursor: checkpoint,
            committed: checkpoint,
            ready: ready,
            cache: merge_cache(state.cache, batch.reads, batch.changes)
        }

        {:ok, if(state.reset, do: write_marker(%{state | reset: false}, checkpoint), else: state)}

      {:error, {:version_conflict, _current}} when attempt < @conflict_retries ->
        resolve_conflict(state, batch, checkpoint, ready, attempt)

      {:error, reason} ->
        {:error, reason, state}
    end
  end

  # Another writer moved the model. If its checkpoint is still ours, only the
  # revision moved and the batch is retried; otherwise that writer has folded
  # past us and this pass restarts from its checkpoint.
  defp resolve_conflict(state, batch, checkpoint, ready, attempt) do
    case read_page(state, state.model, limit: 1) do
      {:ok, model} ->
        revision = model["revision"] || 0

        if state.reset or model["checkpoint"] == state.committed do
          commit(%{state | revision: revision}, batch, checkpoint, ready, attempt + 1)
        else
          {:resync,
           %{
             state
             | revision: revision,
               cursor: model["checkpoint"],
               committed: model["checkpoint"],
               ready: model["ready"] == true,
               cache: %{}
           }}
        end

      {:error, reason} ->
        {:error, reason, state}
    end
  end

  defp write_marker(state, checkpoint, attempt \\ 0) do
    body = %{
      expected_revision: state.marker_revision,
      checkpoint: checkpoint,
      ready: true,
      reset: true,
      changes: %{"generation" => %{"generation" => state.generation}}
    }

    case state.core.commit_read_model(state.tenant_id, state.marker, body) do
      {:ok, revision} ->
        %{state | marker_revision: revision}

      {:error, {:version_conflict, current}} when attempt < @conflict_retries ->
        write_marker(%{state | marker_revision: current}, checkpoint, attempt + 1)

      {:error, reason} ->
        Logger.warning(
          "[ViewFolder] #{state.view} generation marker not written: #{inspect(reason)}",
          tenant_id: state.tenant_id
        )

        state
    end
  end

  ## Helpers

  defp read_page(state, name, opts), do: state.core.read_model_page(state.tenant_id, name, opts)

  defp merge_cache(cache, reads, changes) do
    merged = cache |> Map.merge(reads) |> Map.merge(changes)
    if map_size(merged) > @max_cached_rows, do: changes, else: merged
  end

  defp schedule(%{auto: false} = state, _delay), do: state
  defp schedule(%{timer: timer} = state, _delay) when is_reference(timer), do: state
  defp schedule(state, delay), do: %{state | timer: Process.send_after(self(), :tick, delay)}

  defp next_backoff(0), do: jitter(@base_backoff_ms)
  defp next_backoff(previous), do: jitter(min(previous * 2, @max_backoff_ms))

  defp jitter(ms), do: round(ms * (0.8 + :rand.uniform() * 0.4))

  defp emit(state, name, measurements, metadata) do
    :telemetry.execute(
      [:query_service_ex, :views, name],
      measurements,
      Map.merge(metadata, %{tenant_id: state.tenant_id, view: state.view})
    )
  end

  defp public_status(state) do
    %{
      tenant_id: state.tenant_id,
      view: state.view,
      generation: state.generation,
      revision: state.revision,
      checkpoint: state.committed,
      cursor: state.cursor,
      ready: state.ready,
      rebuilding: state.reset
    }
  end
end
