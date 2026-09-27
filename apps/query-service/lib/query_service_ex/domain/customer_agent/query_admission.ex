defmodule QueryServiceEx.Domain.CustomerAgent.QueryAdmission do
  @moduledoc "Closed canonical query-meter protocol. A receipt is accounting evidence, never source or action authority."
  alias QueryServiceEx.Domain.AgentRun.Event
  @u64 18_446_744_073_709_551_615
  @i64 9_223_372_036_854_775_807
  @protocol "canonical-query-usage-v1"

  def request?(input, now) when is_map(input) do
    keys?(input, ~w(count expected_period fingerprint operation_id)) and
      is_integer(input["count"]) and input["count"] in 1..4 and
      unsigned?(input["expected_period"]) and hash?(input["fingerprint"]) and
      match?({:ok, _}, expires_at(input["operation_id"], now))
  end

  def request?(_, _), do: false

  def snapshot(body) when is_map(body) do
    value = body["snapshot"]

    if keys?(body, ~w(protocol snapshot)) and body["protocol"] == @protocol and
         keys?(value, ~w(managed period quota used)) and unsigned?(value["period"]) and
         unsigned?(value["used"]) and is_integer(value["quota"]) and
         value["quota"] in -1..@i64 and is_boolean(value["managed"]),
       do: {:ok, value},
       else: {:error, :query_usage_unavailable}
  end

  def snapshot(_), do: {:error, :query_usage_unavailable}

  def receipt(body, request, now) when is_map(body) do
    value = body["receipt"]

    with true <- request?(request, now),
         {:ok, expires} <- expires_at(request["operation_id"], now),
         true <- keys?(body, ~w(protocol receipt replayed status)),
         true <- body["protocol"] == @protocol and body["status"] == "admitted",
         true <- is_boolean(body["replayed"]),
         true <- keys?(value, ~w(count expires_at fingerprint operation_id period used)),
         true <- value["operation_id"] == request["operation_id"],
         true <- value["fingerprint"] == request["fingerprint"],
         true <- value["count"] === request["count"],
         true <- value["period"] === request["expected_period"],
         true <- value["expires_at"] === expires,
         true <- unsigned?(value["used"]) and value["used"] >= value["count"] do
      {:ok, %{receipt: value, replayed: body["replayed"]}}
    else
      _ -> {:error, :query_usage_unavailable}
    end
  end

  def receipt(_, _, _), do: {:error, :query_usage_unavailable}

  defp expires_at(id, now) when is_binary(id) and byte_size(id) in 38..56 do
    with [time, nonce] <- String.split(id, ":"),
         {issued, ""} <- Integer.parse(time),
         true <- time == Integer.to_string(issued) and issued >= 0,
         true <- Event.uuid?(nonce) and is_integer(now),
         true <- issued <= now and issued + 3_600 > now and issued + 3_600 <= @i64 do
      {:ok, issued + 3_600}
    else
      _ -> :error
    end
  end

  defp expires_at(_, _), do: :error
  defp unsigned?(value), do: is_integer(value) and value in 0..@u64
  defp keys?(value, keys) when is_map(value), do: Enum.sort(Map.keys(value)) == keys
  defp keys?(_, _), do: false

  defp hash?(value) when is_binary(value) and byte_size(value) == 64,
    do: Regex.match?(~r/\A[0-9a-f]{64}\z/, value)

  defp hash?(_), do: false
end
