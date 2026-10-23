defmodule QueryServiceEx.EgressBudget do
  @moduledoc "Durable response admission shared with the gateway."
  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient

  @max_response 16 * 1024 * 1024

  def reserve(tenant) when is_binary(tenant) and tenant != "" do
    <<a::48, _::4, b::12, _::2, c::62>> = :crypto.strong_rand_bytes(16)
    encoded = Base.encode16(<<a::48, 4::4, b::12, 2::2, c::62>>, case: :lower)
    <<one::binary-8, two::binary-4, three::binary-4, four::binary-4, five::binary-12>> = encoded
    id = Enum.join([one, two, three, four, five], "-")

    case RustCoreClient.change_egress(tenant, %{operation: "reserve", id: id}) do
      {:ok, %{"reserved_bytes" => cap}}
      when is_integer(cap) and cap > 0 and cap <= @max_response ->
        {:ok, %{tenant: tenant, id: id, cap: cap}}

      {:error, status} ->
        {:error, status}

      _ ->
        {:error, 503}
    end
  end

  def reserve(_), do: {:error, 401}

  def complete(reservation, bytes) when bytes >= 0 and bytes <= reservation.cap do
    RustCoreClient.change_egress(reservation.tenant, %{
      operation: "complete",
      id: reservation.id,
      bytes: bytes
    })
  end

  def admit_payload(tenant, payload) do
    with {:ok, reservation} <- reserve(tenant) do
      bytes = payload |> Jason.encode!() |> byte_size()

      if bytes <= reservation.cap do
        case complete(reservation, bytes) do
          {:ok, _} -> :ok
          error -> error
        end
      else
        complete(reservation, 0)
        {:error, 413}
      end
    end
  end
end
