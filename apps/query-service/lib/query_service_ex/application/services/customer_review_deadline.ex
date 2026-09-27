defmodule QueryServiceEx.Application.Services.CustomerReviewDeadline do
  @moduledoc "Bounded supervised review work. Cancellation may follow a committed draft; retry preserves its operation ID."
  alias QueryServiceEx.Application.Services.CustomerReviewWorkDispatcher

  def run(identity, fun) when is_map(identity) do
    CustomerReviewWorkDispatcher.run(identity["tenant_id"], fun)
  end

  def run(_, _), do: {:error, :review_unavailable}
end
