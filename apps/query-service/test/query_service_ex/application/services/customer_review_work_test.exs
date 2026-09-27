defmodule QueryServiceEx.Application.Services.CustomerReviewWorkTest do
  use ExUnit.Case, async: false
  alias QueryServiceEx.Application.Services.CustomerReviewWork, as: Work
  alias QueryServiceEx.Application.Services.CustomerReviewWorkDispatcher, as: Dispatcher
  @dispatcher __MODULE__.Dispatcher
  @tasks __MODULE__.Tasks
  @moduletag timeout: 15_000

  test "tenant and instance bounds refuse immediately without running refused work" do
    pool()
    first = blocked("tenant-a")
    second = blocked("tenant-a")
    owner = self()
    refused = fn -> send(owner, :refused_work_ran) end
    assert {:error, :review_busy} = Dispatcher.run("tenant-a", refused, @dispatcher)
    third = blocked("tenant-b")
    fourth = blocked("tenant-b")
    assert {:error, :review_busy} = Dispatcher.run("tenant-c", refused, @dispatcher)
    refute_receive :refused_work_ran
    finish(first)
    assert :available = Dispatcher.run("tenant-c", fn -> :available end, @dispatcher)
    Enum.each([second, third, fourth], &finish/1)
  end

  test "deadline ends the worker before its slot admits replacement work" do
    pool(80)
    {caller, worker} = blocked("tenant-a")
    monitor = Process.monitor(worker)
    assert {:error, :review_unavailable} = Task.await(caller, 1_000)
    assert_receive {:DOWN, ^monitor, :process, ^worker, :killed}, 1_000
    assert :available = Dispatcher.run("tenant-a", fn -> :available end, @dispatcher)
  end

  test "normal caller exit after abandoning a call cancels the owned worker" do
    pool()
    owner = self()

    {caller, caller_monitor} =
      spawn_monitor(fn ->
        try do
          GenServer.call(@dispatcher, {:run, "tenant-a", waiting(owner)}, 40)
        catch
          :exit, _ -> :ok
        end
      end)

    assert_receive {:working, worker}, 1_000
    worker_monitor = Process.monitor(worker)
    assert_receive {:DOWN, ^caller_monitor, :process, ^caller, :normal}, 1_000
    assert_receive {:DOWN, ^worker_monitor, :process, ^worker, :killed}, 1_000
    assert :available = Dispatcher.run("tenant-a", fn -> :available end, @dispatcher)
  end

  test "abnormal caller departure cancels the worker and linked child tasks" do
    pool()
    owner = self()

    caller =
      spawn(fn ->
        Dispatcher.run(
          "tenant-a",
          fn ->
            child = Task.async(waiting(owner))
            send(owner, {:parent_worker, self()})
            Task.await(child, 5_000)
          end,
          @dispatcher
        )
      end)

    assert_receive {:working, child}, 1_000
    assert_receive {:parent_worker, worker}, 1_000
    worker_monitor = Process.monitor(worker)
    child_monitor = Process.monitor(child)
    Process.exit(caller, :kill)
    assert_receive {:DOWN, ^worker_monitor, :process, ^worker, :killed}, 1_000
    assert_receive {:DOWN, ^child_monitor, :process, ^child, :killed}, 1_000
    assert :available = Dispatcher.run("tenant-a", fn -> :available end, @dispatcher)
  end

  test "worker exceptions, throws and exits return fixed errors without killing admission" do
    pool()
    broker = Process.whereis(@dispatcher)

    for work <- [
          fn -> raise "synthetic-private-error" end,
          fn -> throw("synthetic-private-throw") end,
          fn -> exit("synthetic-private-exit") end,
          fn -> Process.exit(self(), :kill) end
        ] do
      assert {:error, :review_unavailable} = Dispatcher.run("tenant-a", work, @dispatcher)
      assert Process.whereis(@dispatcher) == broker
      assert :available = Dispatcher.run("tenant-a", fn -> :available end, @dispatcher)
    end
  end

  test "dispatcher restart stops old workers before accepting new work" do
    pool()
    {caller, worker} = blocked("tenant-a")
    monitor = Process.monitor(worker)
    broker = Process.whereis(@dispatcher)
    Process.exit(broker, :kill)
    assert {:error, :review_unavailable} = Task.await(caller, 1_000)
    assert_receive {:DOWN, ^monitor, :process, ^worker, _}, 1_000
    assert is_pid(restarted(@dispatcher, broker))
    assert :available = Dispatcher.run("tenant-a", fn -> :available end, @dispatcher)
  end

  test "task supervisor restart also replaces the dispatcher and cancels old work" do
    pool()
    {caller, worker} = blocked("tenant-a")
    monitor = Process.monitor(worker)
    broker = Process.whereis(@dispatcher)
    Process.exit(Process.whereis(@tasks), :kill)
    assert {:error, :review_unavailable} = Task.await(caller, 1_000)
    assert_receive {:DOWN, ^monitor, :process, ^worker, _}, 1_000
    assert is_pid(restarted(@dispatcher, broker))
    assert :available = Dispatcher.run("tenant-a", fn -> :available end, @dispatcher)
  end

  test "independent gateway pools retain their own bounds and results" do
    pool()

    start_supervised!(
      {Work,
       name: __MODULE__.Second,
       tasks: __MODULE__.SecondTasks,
       dispatcher: __MODULE__.SecondDispatcher},
      id: :second_review_pool
    )

    first = blocked("tenant-a")
    second = blocked("tenant-a")
    assert {:error, :review_busy} = Dispatcher.run("tenant-a", fn -> :bad end, @dispatcher)

    assert {:ok, :separate_instance} =
             Dispatcher.run(
               "tenant-a",
               fn -> {:ok, :separate_instance} end,
               __MODULE__.SecondDispatcher
             )

    Enum.each([first, second], &finish/1)
  end

  test "invalid identities and unavailable dispatcher cannot start work" do
    pool()
    owner = self()
    work = fn -> send(owner, :invalid_work_ran) end

    for tenant <- [nil, "", "../other", %{}] do
      assert {:error, :review_unavailable} = Dispatcher.run(tenant, work, @dispatcher)
    end

    assert {:error, :review_unavailable} = Dispatcher.run("tenant-a", nil, @dispatcher)
    assert {:error, :review_unavailable} = Dispatcher.run("tenant-a", work, :missing_review_pool)
    refute_receive :invalid_work_ran
  end

  test "crash formatting excludes identity, inputs and result bodies" do
    private = "synthetic-private-body"

    formatted =
      Dispatcher.format_status(%{
        state: %{jobs: %{make_ref() => %{tenant: private, result: private}}},
        message: {:run, private, private},
        reason: private,
        log: [private]
      })

    assert formatted.state == %{active: 1}
    refute inspect(formatted) =~ private
  end

  test "actual dispatcher crash logs do not disclose private call or active tenant data" do
    pool()
    {caller, worker} = blocked("synthetic-private-tenant")
    monitor = Process.monitor(worker)
    broker = Process.whereis(@dispatcher)

    logs =
      ExUnit.CaptureLog.capture_log(fn ->
        try do
          GenServer.call(@dispatcher, {:unexpected, "synthetic-private-request"}, 1_000)
        catch
          :exit, _ -> :ok
        end

        assert_receive {:DOWN, ^monitor, :process, ^worker, _}, 1_000
        assert is_pid(restarted(@dispatcher, broker))
      end)

    refute logs =~ "synthetic-private"
    assert {:error, :review_unavailable} = Task.await(caller, 1_000)
  end

  defp pool(deadline \\ 5_000) do
    start_supervised!(
      {Work,
       name: __MODULE__.Supervisor, tasks: @tasks, dispatcher: @dispatcher, deadline_ms: deadline}
    )
  end

  defp blocked(tenant) do
    work = waiting(self())
    caller = Task.async(fn -> Dispatcher.run(tenant, work, @dispatcher) end)
    assert_receive {:working, worker}, 1_000
    {caller, worker}
  end

  defp waiting(owner) do
    fn ->
      send(owner, {:working, self()})

      receive do
        :finish -> :finished
      after
        4_000 -> {:error, :fixture_timeout}
      end
    end
  end

  defp finish({caller, worker}) do
    send(worker, :finish)
    assert :finished = Task.await(caller, 1_000)
  end

  defp restarted(name, previous, attempts \\ 100)
  defp restarted(_, _, 0), do: flunk("review supervision did not recover")

  defp restarted(name, previous, attempts) do
    case Process.whereis(name) do
      pid when is_pid(pid) and pid != previous ->
        pid

      _ ->
        Process.sleep(10)
        restarted(name, previous, attempts - 1)
    end
  end
end
