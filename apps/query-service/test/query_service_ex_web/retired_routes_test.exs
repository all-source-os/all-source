defmodule QueryServiceExWeb.RetiredRoutesTest do
  @moduledoc """
  Routes the latest-state contract retired (section 5) because their stores
  carry no tenant key. A route reappearing here would serve another tenant's state.
  """
  use ExUnit.Case, async: true

  alias QueryServiceExWeb.Router

  for {method, path} <- [{"GET", "/api/v1/snapshots"}, {"POST", "/api/query/projected"}] do
    test "#{method} #{path} is not routed" do
      info = Phoenix.Router.route_info(Router, unquote(method), unquote(path), "localhost")
      assert info == :error
    end
  end
end
