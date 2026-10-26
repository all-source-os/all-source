defmodule QueryServiceEx.Views.ViewCatalog do
  @moduledoc """
  Keeps one `QueryServiceEx.Views.ViewFolder` running per defined view.

  Polls Core's admin view listing and starts a folder for every new
  `{tenant, view}`, stops the folder of a deleted view, and restarts a folder
  whose definition moved to a new generation (the new folder rebuilds).

  Starts only when `:views_fold_enabled` is true, because folding writes read
  models into Core.
  """

  use GenServer

  require Logger

  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient
  alias QueryServiceEx.Views.ViewFolder

  @interval_ms 30_000

  @doc """
  Options: `:core`, `:supervisor`, `:interval_ms`, `:folder_opts` (extra options
  for every folder), `:name` and `:enabled` (defaults to the app config).
  """
  def start_link(opts \\ []) do
    enabled =
      Keyword.get_lazy(opts, :enabled, fn ->
        Application.get_env(:query_service_ex, :views_fold_enabled, false)
      end)

    if enabled do
      GenServer.start_link(__MODULE__, opts, name: Keyword.get(opts, :name, __MODULE__))
    else
      :ignore
    end
  end

  @doc "Poll Core now and reconcile; returns the running `{tenant, view} => generation` map."
  def refresh(server \\ __MODULE__), do: GenServer.call(server, :refresh, 30_000)

  @impl true
  def init(opts) do
    state = %{
      core: Keyword.get(opts, :core, RustCoreClient),
      supervisor: Keyword.get(opts, :supervisor, QueryServiceEx.Views.FolderSupervisor),
      interval_ms: Keyword.get(opts, :interval_ms, @interval_ms),
      folder_opts: Keyword.get(opts, :folder_opts, []),
      running: %{}
    }

    send(self(), :poll)
    {:ok, state}
  end

  @impl true
  def handle_call(:refresh, _from, state) do
    state = reconcile(state)
    {:reply, generations(state.running), state}
  end

  @impl true
  def handle_info(:poll, state) do
    state = reconcile(state)
    Process.send_after(self(), :poll, state.interval_ms)
    {:noreply, state}
  end

  def handle_info(_message, state), do: {:noreply, state}

  defp reconcile(state) do
    case state.core.list_all_views() do
      {:ok, views} ->
        desired = desired(views)
        running = state.running |> drop_dead() |> stop_stale(desired, state.supervisor)
        %{state | running: start_missing(running, desired, state)}

      {:error, reason} ->
        Logger.warning("[ViewCatalog] listing views failed: #{inspect(reason)}")
        state
    end
  end

  defp desired(views) do
    for %{"tenant_id" => tenant, "view" => view, "generation" => generation} = entry <- views,
        is_binary(tenant) and is_binary(view) and is_map(entry["definition"]),
        into: %{} do
      {{tenant, view}, {generation, entry["definition"]}}
    end
  end

  defp drop_dead(running) do
    running
    |> Enum.filter(fn {_id, {_generation, pid}} -> Process.alive?(pid) end)
    |> Map.new()
  end

  defp stop_stale(running, desired, supervisor) do
    Enum.reduce(running, running, fn {id, {generation, pid}}, acc ->
      case Map.get(desired, id) do
        {^generation, _definition} ->
          acc

        _ ->
          _ = DynamicSupervisor.terminate_child(supervisor, pid)
          Map.delete(acc, id)
      end
    end)
  end

  defp start_missing(running, desired, state) do
    Enum.reduce(desired, running, fn {{tenant, view} = id, {generation, definition}}, acc ->
      if Map.has_key?(acc, id) do
        acc
      else
        opts =
          [tenant_id: tenant, view: view, generation: generation, definition: definition]
          |> Keyword.merge(state.folder_opts)

        case start_folder(state.supervisor, opts) do
          {:ok, pid} ->
            Map.put(acc, id, {generation, pid})

          {:error, reason} ->
            Logger.warning("[ViewCatalog] #{view} folder not started: #{inspect(reason)}",
              tenant_id: tenant
            )

            acc
        end
      end
    end)
  end

  defp start_folder(supervisor, opts) do
    case DynamicSupervisor.start_child(supervisor, {ViewFolder, opts}) do
      {:ok, pid} -> {:ok, pid}
      {:error, {:already_started, pid}} -> adopt(supervisor, pid, opts)
      {:error, reason} -> {:error, reason}
    end
  end

  # A folder left running by an earlier catalog process is kept only when it
  # folds the current generation; otherwise it is stopped and the next poll
  # starts a fresh one.
  defp adopt(supervisor, pid, opts) do
    if folder_generation(pid) == Keyword.fetch!(opts, :generation) do
      {:ok, pid}
    else
      _ = DynamicSupervisor.terminate_child(supervisor, pid)
      {:error, :replaced_stale_generation}
    end
  end

  defp folder_generation(pid) do
    ViewFolder.status(pid).generation
  catch
    :exit, _reason -> nil
  end

  defp generations(running) do
    Map.new(running, fn {id, {generation, _pid}} -> {id, generation} end)
  end
end
