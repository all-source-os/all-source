defmodule QueryServiceExWeb.FundedChannel do
  @moduledoc "Charges each outbound data message before Phoenix can publish it."
  alias QueryServiceEx.Edition
  alias QueryServiceEx.EgressBudget

  def push(socket, event, payload) do
    case admit(socket, %{event: event, payload: payload}) do
      :ok -> Phoenix.Channel.push(socket, event, payload)
      error -> error
    end
  end

  def admit(socket, payload) do
    if Edition.community?() do
      :ok
    else
      EgressBudget.admit_payload(socket.assigns[:tenant_id], payload)
    end
  end

  def deliver(socket, event, payload) do
    case push(socket, event, payload) do
      :ok -> {:noreply, socket}
      _ -> {:stop, :normal, socket}
    end
  end

  def reply(socket, {status, response} = result) do
    case admit(socket, %{status: status, response: response}) do
      :ok -> {:reply, result, socket}
      _ -> {:stop, :normal, socket}
    end
  end
end
