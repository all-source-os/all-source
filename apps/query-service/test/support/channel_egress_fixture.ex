defmodule QueryServiceEx.ChannelEgressFixture do
  @moduledoc false
  import Plug.Conn
  import ExUnit.Callbacks

  def init(opts), do: opts

  def call(
        %{method: "POST", path_info: ["api", "v1", "tenants", _, "usage", "increment"]} = conn,
        _
      ) do
    send_resp(conn, 200, "{}")
  end

  def call(%{method: "POST", path_info: ["api", "v1", "tenants", _, "egress"]} = conn,
        ledger: ledger
      ) do
    {:ok, body, conn} = read_body(conn)
    command = Jason.decode!(body)

    {status, admission} =
      Agent.get_and_update(ledger, fn state ->
        case command do
          %{"operation" => "reserve", "id" => id} ->
            if state.left > 0 do
              {{200, %{reserved_bytes: state.left}}, put_in(state, [:holds, id], state.left)}
            else
              {{402, %{}}, state}
            end

          %{"operation" => "complete", "id" => id, "bytes" => bytes} ->
            if Map.has_key?(state.holds, id) and bytes <= state.left do
              {{200, %{}},
               %{state | left: state.left - bytes, holds: Map.delete(state.holds, id)}}
            else
              {{503, %{}}, state}
            end

          _ ->
            {{400, %{}}, state}
        end
      end)

    conn
    |> put_resp_content_type("application/json")
    |> send_resp(status, Jason.encode!(%{protocol: "funded-egress-v1", admission: admission}))
  end

  def call(conn, _), do: send_resp(conn, 404, "not found")

  def start do
    ledger = start_supervised!({Agent, fn -> %{left: 1_000_000, holds: %{}} end})
    {:ok, socket} = :gen_tcp.listen(0, [:binary, active: false])
    {:ok, port} = :inet.port(socket)
    :gen_tcp.close(socket)

    start_supervised!(
      {Bandit, plug: {__MODULE__, ledger: ledger}, scheme: :http, port: port, ip: {127, 0, 0, 1}}
    )

    previous = Application.get_env(:query_service_ex, :core_write_url)
    Application.put_env(:query_service_ex, :core_write_url, "http://127.0.0.1:#{port}")

    on_exit(fn ->
      if is_nil(previous),
        do: Application.delete_env(:query_service_ex, :core_write_url),
        else: Application.put_env(:query_service_ex, :core_write_url, previous)
    end)

    ledger
  end
end
