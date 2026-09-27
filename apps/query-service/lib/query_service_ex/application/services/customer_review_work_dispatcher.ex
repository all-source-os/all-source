defmodule QueryServiceEx.Application.Services.CustomerReviewWorkDispatcher do
  @moduledoc "Fail-fast workflow admission; cancellation retains capacity until the owned task exits."
  use GenServer
  alias QueryServiceEx.Domain.AgentRun.Event

  def start_link(options), do: GenServer.start_link(__MODULE__, options, name: options[:name])

  def run(tenant, work, server \\ __MODULE__) do
    if Event.tenant?(tenant) and is_function(work, 0) do
      GenServer.call(server, {:run, tenant, work}, 21_000)
    else
      {:error, :review_unavailable}
    end
  catch
    :exit, _ -> {:error, :review_unavailable}
  end

  @impl true
  def init(options) do
    {:ok, %{tasks: options[:tasks], deadline_ms: options[:deadline_ms], jobs: %{}}}
  end

  # OTP can log exception arguments outside format_status/1; callbacks stop with a fixed reason.
  @impl true
  def handle_call(message, from, state) do
    dispatch_call(message, from, state)
  rescue
    _ -> {:stop, :review_unavailable, {:error, :review_unavailable}, state}
  catch
    _, _ -> {:stop, :review_unavailable, {:error, :review_unavailable}, state}
  end

  @impl true
  def handle_info(message, state) do
    dispatch_info(message, state)
  rescue
    _ -> {:stop, :review_unavailable, state}
  catch
    _, _ -> {:stop, :review_unavailable, state}
  end

  defp dispatch_call({:run, tenant, work}, from, state) do
    cond do
      not Event.tenant?(tenant) or not is_function(work, 0) ->
        {:reply, {:error, :review_unavailable}, state}

      map_size(state.jobs) >= 4 or tenant_count(state, tenant) >= 2 ->
        {:reply, {:error, :review_busy}, state}

      true ->
        start_work(tenant, work, from, state)
    end
  end

  defp dispatch_call(_, _, state),
    do: {:stop, :review_unavailable, {:error, :review_unavailable}, state}

  defp dispatch_info({ref, result}, state) when is_reference(ref) do
    case state.jobs[ref] do
      %{replied: false} = job ->
        {:noreply, put_in(state.jobs[ref], %{job | result: result})}

      _ ->
        {:noreply, state}
    end
  end

  defp dispatch_info({:deadline, ref}, state), do: {:noreply, cancel(state, ref, true)}

  defp dispatch_info({:DOWN, ref, :process, _pid, reason}, state) do
    case Map.pop(state.jobs, ref) do
      {nil, _} ->
        case Enum.find(state.jobs, fn {_, job} -> job.owner == ref end) do
          {task_ref, _} -> {:noreply, cancel(state, task_ref, false)}
          nil -> {:noreply, state}
        end

      {job, remaining} ->
        Process.demonitor(job.owner, [:flush])
        Process.cancel_timer(job.timer)

        unless job.replied do
          result = if reason == :normal, do: job.result, else: {:error, :review_unavailable}
          GenServer.reply(job.from, result)
        end

        {:noreply, %{state | jobs: remaining}}
    end
  end

  defp dispatch_info(_, state), do: {:noreply, state}

  @impl true
  def format_status(status) do
    status
    |> Map.put(:state, %{active: map_size(status.state.jobs)})
    |> Map.put(:message, :redacted)
    |> Map.put(:reason, :redacted)
    |> Map.put(:log, [])
  end

  defp start_work(tenant, work, {caller, _} = from, state) do
    owner = Process.monitor(caller)

    try do
      task = Task.Supervisor.async_nolink(state.tasks, fn -> execute(work) end)
      timer = Process.send_after(self(), {:deadline, task.ref}, state.deadline_ms)

      job = %{
        tenant: tenant,
        task: task,
        owner: owner,
        from: from,
        timer: timer,
        result: {:error, :review_unavailable},
        replied: false
      }

      {:noreply, put_in(state.jobs[task.ref], job)}
    catch
      _, _ ->
        Process.demonitor(owner, [:flush])
        {:reply, {:error, :review_unavailable}, state}
    end
  end

  defp execute(work) do
    work.()
  rescue
    _ -> {:error, :review_unavailable}
  catch
    _, _ -> {:error, :review_unavailable}
  end

  defp cancel(state, ref, reply?) do
    case state.jobs[ref] do
      %{replied: false} = job ->
        Process.exit(job.task.pid, :kill)
        if reply?, do: GenServer.reply(job.from, {:error, :review_unavailable})
        put_in(state.jobs[ref], %{job | replied: true, result: nil})

      _ ->
        state
    end
  end

  defp tenant_count(state, tenant),
    do: Enum.count(state.jobs, fn {_, job} -> job.tenant == tenant end)
end
