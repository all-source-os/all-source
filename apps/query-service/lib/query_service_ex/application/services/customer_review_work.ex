defmodule QueryServiceEx.Application.Services.CustomerReviewWork do
  @moduledoc "Owns bounded review workers and their dispatcher as one restart unit."
  use Supervisor
  alias QueryServiceEx.Application.Services.CustomerReviewWorkDispatcher, as: Dispatcher

  def start_link(options \\ []) do
    Supervisor.start_link(__MODULE__, options, name: Keyword.get(options, :name, __MODULE__))
  end

  @impl true
  def init(options) do
    tasks = Keyword.get(options, :tasks, __MODULE__.Tasks)
    dispatcher = Keyword.get(options, :dispatcher, Dispatcher)
    deadline = Keyword.get(options, :deadline_ms, 20_000)
    true = is_integer(deadline) and deadline in 1..20_000

    children = [
      {Task.Supervisor, name: tasks, max_children: 4},
      {Dispatcher, name: dispatcher, tasks: tasks, deadline_ms: deadline}
    ]

    Supervisor.init(children, strategy: :one_for_all)
  end
end
