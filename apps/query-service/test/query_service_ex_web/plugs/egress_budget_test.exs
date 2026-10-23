defmodule QueryServiceExWeb.Plugs.EgressBudgetTest do
  use ExUnit.Case, async: false
  import Plug.Conn
  import Plug.Test
  alias QueryServiceExWeb.FundedChannel
  alias QueryServiceExWeb.Plugs.EgressBudget

  defmodule Ledger do
    import Plug.Conn
    def init(opts), do: opts

    def call(conn, _) do
      {:ok, body, conn} = read_body(conn)
      command = Jason.decode!(body)

      {status, response} =
        Agent.get_and_update(__MODULE__, fn state ->
          case command do
            %{"operation" => "reserve", "id" => id} ->
              if state.left > 0 do
                {{200, %{"reserved_bytes" => state.left}},
                 put_in(state, [:holds, id], state.left)}
              else
                {{402, %{}}, state}
              end

            %{"operation" => "complete", "id" => id, "bytes" => bytes} ->
              if Map.has_key?(state.holds, id) and bytes <= state.left do
                next = %{
                  state
                  | left: state.left - bytes,
                    holds: Map.delete(state.holds, id),
                    charged: state.charged + bytes
                }

                {{200, %{}}, next}
              else
                {{503, %{}}, state}
              end
          end
        end)

      conn
      |> put_resp_content_type("application/json")
      |> send_resp(status, Jason.encode!(%{protocol: "funded-egress-v1", admission: response}))
    end
  end

  setup do
    start_supervised!(%{
      id: Ledger,
      start: {Agent, :start_link, [fn -> %{left: 6, charged: 0, holds: %{}} end, [name: Ledger]]}
    })

    {:ok, socket} = :gen_tcp.listen(0, [:binary, active: false])
    {:ok, port} = :inet.port(socket)
    :gen_tcp.close(socket)
    start_supervised!({Bandit, plug: Ledger, scheme: :http, port: port, ip: {127, 0, 0, 1}})
    keys = [:core_url, :core_write_url, :edition]
    previous = Enum.map(keys, &{&1, Application.get_env(:query_service_ex, &1)})
    Application.put_env(:query_service_ex, :core_url, "http://127.0.0.1:#{port}")
    Application.put_env(:query_service_ex, :core_write_url, "http://127.0.0.1:#{port}")
    Application.put_env(:query_service_ex, :edition, :enterprise)

    on_exit(fn ->
      Enum.each(previous, fn {key, value} ->
        if is_nil(value),
          do: Application.delete_env(:query_service_ex, key),
          else: Application.put_env(:query_service_ex, key, value)
      end)
    end)

    :ok
  end

  defp admitted do
    conn(:get, "/api/events") |> assign(:tenant_id, "authenticated") |> EgressBudget.call([])
  end

  test "bounded bodies consume allowance then refuse before controller" do
    assert send_resp(admitted(), 200, "abc").resp_body == "abc"
    assert send_resp(admitted(), 200, "abc").resp_body == "abc"
    denied = admitted()
    assert denied.halted and denied.status == 402
    assert Agent.get(Ledger, & &1.charged) == 6
  end

  test "oversized body never reaches adapter and does not spend credit" do
    response = send_resp(admitted(), 200, "private-response")
    assert response.status == 413
    refute response.resp_body =~ "private-response"
    assert Agent.get(Ledger, & &1.charged) == 0
  end

  test "chunked response fails before streaming headers" do
    assert_raise ArgumentError, ~r/funded responses must be bounded/, fn ->
      send_chunked(admitted(), 200)
    end

    assert Agent.get(Ledger, & &1.charged) == 0
  end

  test "WebSocket replies exhaust shared tenant allowance without tuple encoding" do
    Agent.update(Ledger, &%{&1 | left: 100})
    socket = %Phoenix.Socket{assigns: %{tenant_id: "authenticated"}}
    assert {:reply, {:ok, %{value: 1}}, ^socket} = FundedChannel.reply(socket, {:ok, %{value: 1}})
    assert Agent.get(Ledger, & &1.charged) > 0
    Agent.update(Ledger, &%{&1 | left: 0})
    assert {:stop, :normal, ^socket} = FundedChannel.reply(socket, {:ok, %{value: 1}})
  end

  test "billing stays reachable at exhaustion" do
    Agent.update(Ledger, &%{&1 | left: 0})

    response =
      conn(:get, "/api/billing/egress")
      |> assign(:tenant_id, "authenticated")
      |> EgressBudget.call([])

    refute response.halted
  end
end
