defmodule QueryServiceEx.Projections.TenantProjections do
  @moduledoc """
  Per-tenant, QS-owned projection state + folding.

  This is the read-model engine for the per-tenant projections feature
  (`docs/proposals/PER_TENANT_PROJECTIONS.md`). It is intentionally SEPARATE from
  the legacy `QueryServiceEx.Application.Services.ProjectionSync`, which syncs
  Core's global engine state and must not be repurposed.

  ## State

  A single GenServer owns two public ETS tables:

    * `:tenant_projection_state` — `{tenant_id, projection, generation, entity_id} -> state`
    * `:tenant_projection_status` — `{tenant_id, projection} -> :building | :ready`
    * `:tenant_projection_generation` — `{tenant_id, projection} -> active generation`

  State is keyed by `{tenant_id, projection, entity_id}` so one tenant's folded
  read-model never mixes with another's. State is a rebuildable read-model; Core
  remains the durable log of record.

  ## Lifecycle

    * `enable/2` — mark status `:building`, spawn a background backfill Task that
      pages the tenant's whole history from Core and folds it through the
      template's reducer, then flips status to `:ready`. Returns immediately.
    * `disable/2` — drop the tenant's state + status for that projection.
    * Live updates — the GenServer subscribes to the tenant-scoped event topic
      `events:<tenant>:all` and folds new events into the affected enabled
      projections incrementally.

  ## Isolation

  Every key carries `tenant_id`; the live subscription is to a tenant-scoped
  PubSub topic only. There is no cross-tenant read or write path.
  """

  use GenServer
  require Logger

  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient
  alias QueryServiceEx.Projections.Catalog
  alias QueryServiceEx.Projections.ReplayRecord
  alias QueryServiceEx.Projections.TrackedReplays
  alias QueryServiceExWeb.ChannelBroadcaster

  @state_table :tenant_projection_state
  @status_table :tenant_projection_status
  @generation_table :tenant_projection_generation
  @pubsub QueryServiceEx.PubSub

  # Backfill paging: bounded pages, loop until a short page.
  @backfill_page_size 1_000
  @max_backfill_pages 10_000

  # -- Client API --

  def start_link(opts \\ []) do
    GenServer.start_link(__MODULE__, opts, name: __MODULE__)
  end

  @doc """
  Initialize the ETS tables. Safe to call before the GenServer starts (the
  GenServer also ensures them in `init/1`).
  """
  def init_tables do
    ensure_table(@state_table)
    ensure_table(@status_table)
    ensure_table(@generation_table)
    :ok
  end

  @doc """
  Enable a template for a tenant: set status `:building` and kick off a
  background backfill. Returns `:ok` immediately, or `{:error, :unknown_template}`.
  """
  @spec enable(String.t(), String.t()) :: :ok | {:error, :unknown_template}
  def enable(tenant_id, template_name)
      when is_binary(tenant_id) and is_binary(template_name) do
    case Catalog.fetch(template_name) do
      {:ok, _template} ->
        GenServer.call(__MODULE__, {:enable, tenant_id, template_name})

      :error ->
        {:error, :unknown_template}
    end
  end

  @doc """
  Disable a projection for a tenant: drop its folded state + status.
  """
  @spec disable(String.t(), String.t()) :: :ok
  def disable(tenant_id, template_name)
      when is_binary(tenant_id) and is_binary(template_name) do
    GenServer.call(__MODULE__, {:disable, tenant_id, template_name})
  end

  @doc """
  Read folded state for `{tenant_id, projection, entity_id}`.

  Returns `{:ok, state}` or `{:error, :not_found}`. Reads straight from ETS.
  """
  @spec get_state(String.t(), String.t(), String.t()) :: {:ok, term()} | {:error, :not_found}
  def get_state(tenant_id, projection, entity_id)
      when is_binary(tenant_id) and is_binary(projection) and is_binary(entity_id) do
    with {:ok, generation} <- lookup(@generation_table, {tenant_id, projection}),
         {:ok, state} <- lookup(@state_table, {tenant_id, projection, generation, entity_id}) do
      {:ok, state}
    else
      :error -> {:error, :not_found}
    end
  end

  @doc """
  Current status of an enabled projection: `:building`, `:ready`, or `nil` if not
  enabled in QS state.
  """
  @spec status(String.t(), String.t()) :: :building | :ready | nil
  def status(tenant_id, projection) when is_binary(tenant_id) and is_binary(projection) do
    case lookup(@status_table, {tenant_id, projection}) do
      {:ok, status} -> status
      :error -> nil
    end
  end

  @doc """
  Summary of a tenant's QS-tracked projections (those with a status entry).

  Returns a list of `%{name: ..., status: "building" | "ready", entity_count: n}`.
  """
  @spec list(String.t()) :: [map()]
  def list(tenant_id) when is_binary(tenant_id) do
    statuses =
      safe_select(@status_table, [
        {{{tenant_id, :"$1"}, :"$2"}, [], [{{:"$1", :"$2"}}]}
      ])

    Enum.map(statuses, fn {name, status} ->
      %{
        name: name,
        status: Atom.to_string(status),
        entity_count: entity_count(tenant_id, name)
      }
    end)
  end

  @doc """
  Rebuild one enabled projection from this tenant's immutable event history.

  History folds into isolated memory first. Current read-model remains available
  until the fold succeeds, then the new state replaces it in one GenServer turn.
  """
  @spec rebuild(String.t(), String.t()) :: {:ok, map()} | {:error, term()}
  def rebuild(tenant_id, template_name)
      when is_binary(tenant_id) and is_binary(template_name) do
    GenServer.call(__MODULE__, {:rebuild, tenant_id, template_name})
  end

  @doc false
  def rebuild_tracked(record) do
    GenServer.call(__MODULE__, {:rebuild_tracked, record})
  end

  @doc "List replay jobs belonging to one tenant."
  @spec list_replays(String.t()) :: [map()]
  def list_replays(tenant_id) when is_binary(tenant_id) do
    GenServer.call(__MODULE__, {:list_replays, tenant_id})
  end

  @doc "Read one replay job, scoped to its tenant."
  @spec get_replay(String.t(), String.t()) :: {:ok, map()} | {:error, :not_found}
  def get_replay(tenant_id, replay_id)
      when is_binary(tenant_id) and is_binary(replay_id) do
    GenServer.call(__MODULE__, {:get_replay, tenant_id, replay_id})
  end

  @doc "Cancel a running replay without replacing its current read-model."
  @spec cancel_replay(String.t(), String.t()) :: {:ok, map()} | {:error, term()}
  def cancel_replay(tenant_id, replay_id)
      when is_binary(tenant_id) and is_binary(replay_id) do
    GenServer.call(__MODULE__, {:cancel_replay, tenant_id, replay_id})
  end

  @doc "Remove a completed, failed, or cancelled replay from tenant history."
  @spec delete_replay(String.t(), String.t()) :: :ok | {:error, term()}
  def delete_replay(tenant_id, replay_id)
      when is_binary(tenant_id) and is_binary(replay_id) do
    GenServer.call(__MODULE__, {:delete_replay, tenant_id, replay_id})
  end

  # -- Server callbacks --

  @impl GenServer
  def init(_opts) do
    init_tables()

    # Live updates fold into all tenants' enabled projections. We subscribe to a
    # per-tenant topic lazily the first time a tenant enables anything (a tenant
    # may not exist yet at boot). Track which tenant topics we are subscribed to.
    {:ok, %{subscribed_tenants: MapSet.new(), builds: %{}, replays: %{}}}
  end

  @impl GenServer
  def handle_call({:enable, tenant_id, template_name}, _from, state) do
    {:ok, template} = Catalog.fetch(template_name)
    state = cancel_tracked_build(state, {tenant_id, template_name})

    :ets.insert(@status_table, {{tenant_id, template_name}, :building})
    state = ensure_tenant_subscription(state, tenant_id)

    state = start_build(state, tenant_id, template, nil)

    {:reply, :ok, state}
  end

  @impl GenServer
  def handle_call({:rebuild, tenant_id, template_name}, _from, state) do
    begin_rebuild(state, tenant_id, template_name, nil)
  end

  @impl GenServer
  def handle_call({:rebuild_tracked, record}, _from, state) do
    if ReplayRecord.valid?(record, record["tenant_id"], record["operation_id"]) and
         record["status"] == "unknown" do
      begin_rebuild(state, record["tenant_id"], record["projection_name"], record)
    else
      {:reply, {:error, :invalid_replay}, state}
    end
  end

  @impl GenServer
  def handle_call({:list_replays, tenant_id}, _from, state) do
    replays =
      state.replays
      |> Map.values()
      |> Enum.filter(&(&1.tenant_id == tenant_id))
      |> Enum.sort_by(& &1.started_at, :desc)
      |> Enum.map(&public_replay/1)

    {:reply, replays, state}
  end

  @impl GenServer
  def handle_call({:get_replay, tenant_id, replay_id}, _from, state) do
    reply =
      case Map.get(state.replays, replay_id) do
        %{tenant_id: ^tenant_id} = replay -> {:ok, public_replay(replay)}
        _ -> {:error, :not_found}
      end

    {:reply, reply, state}
  end

  @impl GenServer
  def handle_call({:cancel_replay, tenant_id, replay_id}, _from, state) do
    case Map.get(state.replays, replay_id) do
      %{tenant_id: ^tenant_id, status: status} = replay when status in ["pending", "running"] ->
        now = now_iso8601()
        replay = %{replay | status: "cancelled", completed_at: now, updated_at: now}
        key = {tenant_id, replay.projection_name}
        :ets.insert(@status_table, {key, :ready})

        state = update_replay(state, replay_id, fn _ -> replay end)
        state = %{state | builds: Map.delete(state.builds, key)}

        {:reply, {:ok, public_replay(replay)}, state}

      %{tenant_id: ^tenant_id} ->
        {:reply, {:error, :not_running}, state}

      _ ->
        {:reply, {:error, :not_found}, state}
    end
  end

  @impl GenServer
  def handle_call({:delete_replay, tenant_id, replay_id}, _from, state) do
    case Map.get(state.replays, replay_id) do
      %{tenant_id: ^tenant_id, status: status}
      when status in ["completed", "failed", "cancelled"] ->
        {:reply, :ok, %{state | replays: Map.delete(state.replays, replay_id)}}

      %{tenant_id: ^tenant_id} ->
        {:reply, {:error, :still_running}, state}

      _ ->
        {:reply, {:error, :not_found}, state}
    end
  end

  @impl GenServer
  def handle_call({:disable, tenant_id, template_name}, _from, state) do
    state = cancel_tracked_build(state, {tenant_id, template_name})
    # Drop the status, then all per-entity state rows for this projection.
    :ets.delete(@status_table, {tenant_id, template_name})

    :ets.delete(@generation_table, {tenant_id, template_name})
    safe_match_delete(@state_table, {{tenant_id, template_name, :_, :_}, :_})

    {:reply, :ok, %{state | builds: Map.delete(state.builds, {tenant_id, template_name})}}
  end

  @impl GenServer
  def handle_cast({:build_progress, tenant_id, projection, token, processed, total}, state) do
    key = {tenant_id, projection}

    state =
      case Map.get(state.builds, key) do
        %{token: ^token, replay_id: replay_id} when is_binary(replay_id) ->
          update_replay(state, replay_id, fn replay ->
            total_events = total || replay.total_events
            percentage = progress_percentage(processed, total_events)
            now = now_iso8601()

            %{
              replay
              | processed_events: processed,
                total_events: total_events,
                progress_percentage: percentage,
                events_per_second: events_per_second(processed, replay.started_at, now),
                updated_at: now
            }
          end)

        _ ->
          state
      end

    {:noreply, state}
  end

  @impl GenServer
  def handle_cast({:build_done, tenant_id, projection, token, template, shadow, processed}, state) do
    key = {tenant_id, projection}

    case Map.get(state.builds, key) do
      %{token: ^token, replay_id: replay_id, buffer: buffer} ->
        final_shadow = Enum.reduce(Enum.reverse(buffer), shadow, &fold_into_map(&2, template, &1))
        replace_projection_state(tenant_id, projection, token, final_shadow)
        :ets.insert(@status_table, {key, :ready})

        state = %{state | builds: Map.delete(state.builds, key)}

        state =
          if is_binary(replay_id) do
            update_replay(state, replay_id, fn replay ->
              now = now_iso8601()

              %{
                replay
                | status: "completed",
                  completed_at: now,
                  updated_at: now,
                  processed_events: processed,
                  total_events: processed,
                  progress_percentage: 100.0,
                  events_per_second: events_per_second(processed, replay.started_at, now)
              }
            end)
          else
            state
          end

        Logger.info("[TenantProjections] atomic rebuild ready",
          tenant_id: tenant_id,
          projection: projection,
          events: processed
        )

        {:noreply, state}

      _ ->
        # Disabled, cancelled, or superseded while task was folding. Discard shadow.
        {:noreply, state}
    end
  end

  @impl GenServer
  def handle_cast({:build_failed, tenant_id, projection, token, reason}, state) do
    key = {tenant_id, projection}

    case Map.get(state.builds, key) do
      %{token: ^token, replay_id: replay_id} ->
        # Existing state never moved, so it remains safe to serve.
        :ets.insert(@status_table, {key, :ready})
        state = %{state | builds: Map.delete(state.builds, key)}

        state =
          if is_binary(replay_id) do
            update_replay(state, replay_id, fn replay ->
              now = now_iso8601()

              %{
                replay
                | status: "failed",
                  completed_at: now,
                  updated_at: now,
                  error_message: to_string_reason(reason)
              }
            end)
          else
            state
          end

        {:noreply, state}

      _ ->
        {:noreply, state}
    end
  end

  # Live event from the tenant-scoped "all" topic — fold into each enabled
  # projection for that tenant.
  @impl GenServer
  def handle_info({:new_event, event}, state) when is_map(event) do
    case event["tenant_id"] || event[:tenant_id] do
      tenant_id when is_binary(tenant_id) ->
        fold_live_event(tenant_id, event)
        {:noreply, buffer_live_event(state, tenant_id, event)}

      _ ->
        {:noreply, state}
    end
  end

  @impl GenServer
  def handle_info({:cleanup_generation, tenant_id, projection, generation}, state) do
    safe_match_delete(@state_table, {{tenant_id, projection, generation, :_}, :_})
    {:noreply, state}
  end

  @impl GenServer
  def handle_info(_msg, state), do: {:noreply, state}

  # -- Internal: subscriptions --

  defp ensure_tenant_subscription(state, tenant_id) do
    if MapSet.member?(state.subscribed_tenants, tenant_id) do
      state
    else
      # Tenant-scoped topic ONLY (events:<tenant>:all). Never a global topic —
      # the tenant-isolation gate forbids it and it would leak cross-tenant.
      Phoenix.PubSub.subscribe(@pubsub, "events:#{tenant_id}:all")
      %{state | subscribed_tenants: MapSet.put(state.subscribed_tenants, tenant_id)}
    end
  end

  # -- Internal: live folding --

  defp fold_live_event(tenant_id, event) do
    @status_table
    |> safe_select([{{{tenant_id, :"$1"}, :"$2"}, [], [{{:"$1", :"$2"}}]}])
    |> Enum.each(fn {projection, _status} ->
      case Catalog.fetch(projection) do
        {:ok, template} -> apply_event(tenant_id, projection, template, event)
        :error -> :ok
      end
    end)
  end

  defp apply_event(tenant_id, projection, template, event) do
    case lookup(@generation_table, {tenant_id, projection}) do
      {:ok, generation} ->
        entity_id = template.entity_key.(event)
        key = {tenant_id, projection, generation, entity_id}

        current =
          case lookup(@state_table, key) do
            {:ok, s} -> s
            :error -> template.initial
          end

        new_state = template.reduce.(current, event)
        :ets.insert(@state_table, {key, new_state})

        broadcast_state(tenant_id, projection, entity_id, new_state)
        new_state

      :error ->
        :ok
    end
  end

  defp broadcast_state(tenant_id, projection, entity_id, state) do
    ChannelBroadcaster.broadcast_projection_update(tenant_id, projection, entity_id, state)
  rescue
    _ -> :ok
  end

  # -- Internal: atomic builds --

  defp begin_rebuild(state, tenant_id, template_name, tracking) do
    key = {tenant_id, template_name}

    cond do
      not Catalog.valid?(template_name) ->
        {:reply, {:error, :unknown_template}, state}

      status(tenant_id, template_name) == nil ->
        {:reply, {:error, :projection_not_enabled}, state}

      Map.has_key?(state.builds, key) ->
        {:reply, {:error, :already_running}, state}

      true ->
        {:ok, template} = Catalog.fetch(template_name)
        replay = new_replay(template_name, tracking)
        :ets.insert(@status_table, {key, :building})

        state =
          state
          |> ensure_tenant_subscription(tenant_id)
          |> put_in([:replays, replay.replay_id], Map.put(replay, :tenant_id, tenant_id))
          |> start_build(tenant_id, template, replay.replay_id, replay_cutoff(tracking))

        {:reply, {:ok, public_replay(replay)}, state}
    end
  end

  defp start_build(state, tenant_id, template, replay_id, cutoff \\ nil) do
    server = self()
    token = make_ref()
    key = {tenant_id, template.name}
    cutoff = cutoff || now_iso8601()

    Task.Supervisor.start_child(QueryServiceEx.Projections.BackfillSupervisor, fn ->
      build_projection(server, tenant_id, template, token, cutoff)
    end)

    put_in(state, [:builds, key], %{
      token: token,
      replay_id: replay_id,
      cutoff: cutoff,
      buffer: []
    })
  end

  defp build_projection(server, tenant_id, template, token, cutoff) do
    projection = template.name

    Logger.info("[TenantProjections] atomic rebuild start",
      tenant_id: tenant_id,
      projection: projection
    )

    ctx = %{
      server: server,
      tenant_id: tenant_id,
      template: template,
      token: token,
      cutoff: cutoff
    }

    case fold_history(ctx, %{offset: 0, page_no: 0, shadow: %{}, count: 0}) do
      {:ok, shadow, processed} ->
        GenServer.cast(
          server,
          {:build_done, tenant_id, projection, token, template, shadow, processed}
        )

      {:error, reason} ->
        GenServer.cast(server, {:build_failed, tenant_id, projection, token, reason})
    end
  rescue
    error ->
      Logger.error(
        "[TenantProjections] atomic rebuild crashed: #{inspect(error)}",
        tenant_id: tenant_id,
        projection: template.name
      )

      GenServer.cast(server, {:build_failed, tenant_id, template.name, token, error})
  end

  # `ctx` carries what stays fixed for the whole backfill (server, tenant_id,
  # template, token, cutoff); `state` carries the per-page cursor (offset,
  # page_no, shadow, count). Nine positional parameters was both a Credo
  # failure and genuinely easy to call wrong — two of them were adjacent
  # integers, so a transposed `offset`/`page_no` would have type-checked fine
  # and silently mis-paged the backfill.
  defp fold_history(_ctx, %{page_no: page_no}) when page_no >= @max_backfill_pages do
    {:error, :max_backfill_pages_reached}
  end

  defp fold_history(ctx, state) do
    params = %{limit: @backfill_page_size, offset: state.offset, order: "asc", as_of: ctx.cutoff}

    case query_events_page(ctx.tenant_id, params) do
      {:ok, events, total} when is_list(events) ->
        next_shadow = Enum.reduce(events, state.shadow, &fold_into_map(&2, ctx.template, &1))
        processed = state.count + length(events)

        GenServer.cast(
          ctx.server,
          {:build_progress, ctx.tenant_id, ctx.template.name, ctx.token, processed, total}
        )

        if length(events) < @backfill_page_size do
          {:ok, next_shadow, processed}
        else
          fold_history(ctx, %{
            offset: state.offset + length(events),
            page_no: state.page_no + 1,
            shadow: next_shadow,
            count: processed
          })
        end

      {:error, reason} ->
        Logger.warning(
          "[TenantProjections] backfill page failed at offset #{state.offset}: #{inspect(reason)}",
          tenant_id: ctx.tenant_id,
          projection: ctx.template.name
        )

        {:error, reason}
    end
  end

  defp fold_into_map(shadow, template, event) do
    entity_id = template.entity_key.(event)
    current = Map.get(shadow, entity_id, template.initial)
    Map.put(shadow, entity_id, template.reduce.(current, event))
  end

  # Defaults to Core's tenant-scoped event query; tests can inject a list source.
  defp query_events_page(tenant_id, params) do
    case Application.get_env(:query_service_ex, :tenant_projection_query_fun) do
      fun when is_function(fun, 2) -> normalize_injected(fun.(tenant_id, params))
      _ -> normalize_core(RustCoreClient.query_events_page(tenant_id, params))
    end
  end

  defp normalize_injected({:ok, events}) when is_list(events), do: {:ok, events, nil}
  defp normalize_injected(other), do: other

  defp normalize_core({:ok, body}) do
    events = first_key(body, ["events", :events]) || []
    total = first_key(body, ["total_count", :total_count, "total", :total])
    {:ok, events, total}
  end

  defp normalize_core(error), do: error

  # Core answers with string keys, injected test sources with atoms, and the
  # total has carried two different names. Each `||` in the old inline chain
  # counted as a branch, which is what pushed this function past Credo's
  # complexity ceiling; a lookup over a key list is the same behaviour without
  # the branching (only nil/false are falsy in Elixir, so a 0 total still wins).
  defp first_key(body, keys), do: Enum.find_value(keys, &Map.get(body, &1))

  defp buffer_live_event(state, tenant_id, event) do
    builds =
      Enum.reduce(state.builds, state.builds, fn
        {{^tenant_id, projection} = key, build}, acc ->
          case Catalog.fetch(projection) do
            {:ok, _template} ->
              if after_cutoff?(event, build.cutoff) do
                Map.put(acc, key, %{build | buffer: [event | build.buffer]})
              else
                acc
              end

            :error ->
              acc
          end

        _, acc ->
          acc
      end)

    %{state | builds: builds}
  end

  defp replace_projection_state(tenant_id, projection, generation, shadow) do
    Enum.each(shadow, fn {entity_id, projection_state} ->
      :ets.insert(
        @state_table,
        {{tenant_id, projection, generation, entity_id}, projection_state}
      )

      broadcast_state(tenant_id, projection, entity_id, projection_state)
    end)

    previous =
      case lookup(@generation_table, {tenant_id, projection}) do
        {:ok, value} -> value
        :error -> nil
      end

    # One pointer write publishes every new row together from readers' view.
    :ets.insert(@generation_table, {{tenant_id, projection}, generation})

    if previous do
      Process.send_after(self(), {:cleanup_generation, tenant_id, projection, previous}, 60_000)
    end
  end

  defp new_replay(projection_name, tracking) do
    now = now_iso8601()

    %{
      replay_id: if(tracking, do: tracking["replay_id"], else: generate_id()),
      tracking: tracking,
      projection_name: projection_name,
      status: "running",
      started_at: now,
      updated_at: now,
      completed_at: nil,
      total_events: 0,
      processed_events: 0,
      failed_events: 0,
      progress_percentage: 0.0,
      events_per_second: 0.0,
      error_message: nil
    }
  end

  defp public_replay(replay), do: Map.drop(replay, [:tenant_id, :tracking])

  defp update_replay(state, replay_id, fun) do
    case Map.fetch(state.replays, replay_id) do
      {:ok, replay} ->
        next = fun.(replay)
        persist_terminal(next)
        %{state | replays: Map.put(state.replays, replay_id, next)}

      :error ->
        state
    end
  end

  defp persist_terminal(%{tracking: record, status: status} = replay)
       when is_map(record) and status in ["completed", "failed", "cancelled"] do
    Task.Supervisor.start_child(QueryServiceEx.Projections.BackfillSupervisor, fn ->
      TrackedReplays.persist(record, replay)
    end)

    :ok
  catch
    :exit, _ -> :ok
  end

  defp persist_terminal(_), do: :ok

  defp replay_cutoff(nil), do: nil
  defp replay_cutoff(record), do: record["cutoff"]

  defp cancel_tracked_build(state, key) do
    with %{replay_id: id} when is_binary(id) <- Map.get(state.builds, key),
         %{tracking: record} when is_map(record) <- Map.get(state.replays, id) do
      update_replay(state, id, fn replay ->
        %{replay | status: "cancelled", completed_at: now_iso8601(), updated_at: now_iso8601()}
      end)
    else
      _ -> state
    end
  end

  defp progress_percentage(_processed, total) when total in [nil, 0], do: 0.0
  defp progress_percentage(processed, total), do: min(processed / total * 100.0, 99.9)

  defp events_per_second(processed, started_at, updated_at) do
    with {:ok, started, _offset} <- DateTime.from_iso8601(started_at),
         {:ok, updated, _offset} <- DateTime.from_iso8601(updated_at) do
      elapsed_seconds = max(DateTime.diff(updated, started, :millisecond) / 1_000, 0.001)
      Float.round(processed / elapsed_seconds, 1)
    else
      _ -> 0.0
    end
  end

  defp generate_id do
    :crypto.strong_rand_bytes(16)
    |> Base.url_encode64(padding: false)
  end

  defp now_iso8601, do: DateTime.utc_now() |> DateTime.to_iso8601()

  defp after_cutoff?(event, cutoff) do
    case event["timestamp"] || event[:timestamp] do
      timestamp when is_binary(timestamp) -> timestamp > cutoff
      _ -> true
    end
  end

  defp to_string_reason(reason) when is_binary(reason), do: reason
  defp to_string_reason(reason), do: inspect(reason)

  # -- Internal: ETS helpers --

  defp ensure_table(table) do
    if :ets.whereis(table) == :undefined do
      :ets.new(table, [:set, :public, :named_table, read_concurrency: true])
    end
  end

  defp lookup(table, key) do
    case :ets.lookup(table, key) do
      [{^key, value}] -> {:ok, value}
      [] -> :error
    end
  rescue
    ArgumentError -> :error
  end

  defp safe_select(table, spec) do
    :ets.select(table, spec)
  rescue
    ArgumentError -> []
  end

  defp safe_match_delete(table, pattern) do
    :ets.match_delete(table, pattern)
    :ok
  rescue
    ArgumentError -> :ok
  end

  defp entity_count(tenant_id, projection) do
    case lookup(@generation_table, {tenant_id, projection}) do
      {:ok, generation} ->
        safe_select(@state_table, [
          {{{tenant_id, projection, generation, :"$1"}, :_}, [], [true]}
        ])
        |> length()

      :error ->
        0
    end
  end
end
