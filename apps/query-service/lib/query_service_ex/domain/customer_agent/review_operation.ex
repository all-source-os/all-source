defmodule QueryServiceEx.Domain.CustomerAgent.ReviewOperation do
  @moduledoc "Immutable one-hour retry identity for metered customer evidence work."
  alias QueryServiceEx.Domain.AgentRun.Event
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOwner, as: Owner
  @purposes ~w(source.share review.prepare review.read review.result)

  def issued_at(value) when is_binary(value) and byte_size(value) in 38..53 do
    with [time, nonce] <- String.split(value, ":"),
         {issued, ""} <- Integer.parse(time),
         true <- time == Integer.to_string(issued) and Owner.timestamp?(issued),
         true <- Event.uuid?(nonce) do
      {:ok, issued}
    else
      _ -> {:error, :invalid_operation}
    end
  end

  def issued_at(_), do: {:error, :invalid_operation}

  def valid_at?(value, now) do
    case issued_at(value) do
      {:ok, issued} -> Owner.timestamp?(now) and issued <= now and now < issued + 3_600
      _ -> false
    end
  end

  def request(owner, purpose, operation, intent, count, now) do
    if Owner.valid?(owner) and purpose in @purposes and valid_at?(operation, now) and
         is_integer(count) and count in 1..4 do
      {:ok, issued} = issued_at(operation)
      hash = Owner.digest(["customer-query-operation-v1", Owner.take(owner), purpose, operation])

      <<a::binary-size(8), b::binary-size(4), c::binary-size(4), d::binary-size(4),
        e::binary-size(12), _::binary>> = hash

      {:ok,
       %{
         "operation_id" => "#{issued}:#{a}-#{b}-#{c}-#{d}-#{e}",
         "fingerprint" => Owner.digest([Owner.take(owner), purpose, operation, intent, count]),
         "count" => count
       }}
    else
      {:error, :invalid_operation}
    end
  end
end
