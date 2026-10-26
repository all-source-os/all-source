defmodule QueryServiceExWeb.ConsumerAccessController do
  use Phoenix.Controller, formats: [:json]

  alias QueryServiceEx.Infrastructure.Adapters.ConsumerAccessProbe

  def show(conn, _params) do
    result = ConsumerAccessProbe.result()

    conn
    |> put_status(if(result.ok, do: 200, else: 503))
    |> json(result)
  end
end
