defmodule QueryServiceEx.Application.Services.CustomerReviewDeadline do
  @moduledoc "End-to-end bound for internal review work. Timeout may follow a committed draft; retry uses its idempotency key."
  def run(fun) do
    task =
      Task.async(fn ->
        try do
          fun.()
        rescue
          _ -> {:error, :review_unavailable}
        catch
          _, _ -> {:error, :review_unavailable}
        end
      end)

    case Task.yield(task, 20_000) || Task.shutdown(task, :brutal_kill) do
      {:ok, result} -> result
      _ -> {:error, :review_unavailable}
    end
  end
end
