defmodule QueryServiceEx.Domain.EventHistory do
  @moduledoc "Exact checkout-history selection and permitted evidence fields."
  alias QueryServiceEx.Domain.AgentRun.Event

  def select(%{"entity" => entity} = args, clock) do
    with true <- entity?(entity),
         {:ok, first, last} <- bounds(Map.delete(args, "entity"), clock) do
      {:ok,
       %{
         "entity_id" => entity,
         "since" => first,
         "until" => last
       }}
    else
      _ -> {:error, :invalid_request}
    end
  end

  def select(_, _), do: {:error, :invalid_request}

  defp bounds(params, now) when map_size(params) == 0,
    do: {:ok, DateTime.to_iso8601(DateTime.add(now, -86_400)), DateTime.to_iso8601(now)}

  defp bounds(%{"from" => first, "to" => last} = args, now) when map_size(args) == 2 do
    with {:ok, a, low} <- instant(first),
         {:ok, b, high} <- instant(last),
         true <- low < high and high <= DateTime.to_unix(now, :nanosecond),
         true <- high - low <= 31 * 86_400_000_000_000 do
      {:ok, a, b}
    end
  end

  defp bounds(_, _), do: :error

  def entity?(value) when is_binary(value) and byte_size(value) in 1..128 do
    String.valid?(value) and value == String.trim(value) and value not in [".", ".."] and
      not Regex.match?(~r/[\p{C}\/\\]/u, value)
  end

  def entity?(_), do: false

  def timeline(page, workspace, filter, origin) do
    with {:ok, rows} <- envelope(page, workspace, filter),
         true <- length(rows) <= 21,
         {:ok, events} <- project_rows(rows, workspace["id"], filter, origin),
         true <- events == Enum.sort_by(events, &rank/1, :desc),
         true <- Enum.uniq_by(events, & &1["id"]) == events do
      {:ok,
       %{
         "workspace" => workspace,
         "entity" => filter["entity_id"],
         "from" => filter["since"],
         "to" => filter["until"],
         "coverage" =>
           if(
             length(events) > 20 or page["truncated"] or page["coverage"] != "complete" or
               not covers_end?(page),
             do: "partial",
             else: "complete"
           ),
         "events" => Enum.take(events, 20)
       }}
    else
      _ -> {:error, :source_unavailable}
    end
  end

  def evidence(page, workspace, id, origin) do
    with {:ok, rows} <- envelope(page, workspace, %{"event_id" => id}),
         true <- page["truncated"] == false do
      case rows do
        [] ->
          if page["coverage"] == "complete",
            do: {:error, :not_found},
            else: {:error, :source_unavailable}

        [row] ->
          with {:ok, event} <- project(row, workspace["id"], %{"event_id" => id}, origin),
               do: {:ok, %{"workspace" => workspace, "event" => event}}

        _ ->
          {:error, :source_unavailable}
      end
    else
      _ -> {:error, :source_unavailable}
    end
  end

  defp envelope(page, workspace, filter) when is_map(page) do
    with true <-
           page["protocol"] == "retained-history-v1" and page["tenant_id"] == workspace["id"],
         true <-
           page["entity_id"] == filter["entity_id"] and page["event_id"] == filter["event_id"],
         true <-
           same_instant?(page["since"], filter["since"]) and
             same_instant?(page["until"], filter["until"]),
         {:ok, _, point} <- instant(page["read_point"]),
         true <- page["coverage"] in ["complete", "partial"] and is_boolean(page["truncated"]),
         rows when is_list(rows) and length(rows) <= 21 <- page["events"],
         true <- Enum.all?(rows, &at_snapshot?(&1, point)) do
      {:ok, rows}
    else
      _ -> {:error, :source_unavailable}
    end
  end

  defp envelope(_, _, _), do: {:error, :source_unavailable}

  defp project_rows(rows, tenant, filter, origin) do
    Enum.reduce_while(rows, {:ok, []}, fn row, {:ok, acc} ->
      case project(row, tenant, filter, origin) do
        {:ok, event} -> {:cont, {:ok, [event | acc]}}
        error -> {:halt, error}
      end
    end)
    |> case do
      {:ok, reversed} -> {:ok, Enum.reverse(reversed)}
      error -> error
    end
  end

  defp project(
         %{"id" => id, "tenant_id" => tenant, "entity_id" => entity, "timestamp" => timestamp} =
           row,
         tenant,
         filter,
         origin
       ) do
    with true <- Event.uuid?(id) and entity?(entity),
         {:ok, date, nanos} <- instant(timestamp),
         true <- matches?(filter, id, entity, nanos) do
      {:ok,
       Map.merge(change(row), %{
         "id" => id,
         "timestamp" => date,
         "entity" => entity,
         "url" =>
           origin <>
             "/event-history/workspaces/" <> URI.encode_www_form(tenant) <> "/events/" <> id
       })}
    else
      _ -> {:error, :source_unavailable}
    end
  end

  defp project(_, _, _, _), do: {:error, :source_unavailable}

  defp matches?(%{"event_id" => expected}, id, _, _), do: expected == id

  defp matches?(%{"entity_id" => entity, "since" => first, "until" => last}, _, entity, date) do
    {:ok, _, low} = instant(first)
    {:ok, _, high} = instant(last)
    date >= low and date < high
  end

  defp matches?(_, _, _, _), do: false

  defp rank(%{"timestamp" => timestamp, "id" => id}) do
    {:ok, _, nanos} = instant(timestamp)
    {nanos, id}
  end

  defp change(%{"event_type" => "deployment.released.v1", "payload" => %{"version" => v}}),
    do:
      if(version?(v), do: %{"kind" => "released", "version" => v}, else: %{"kind" => "recorded"})

  defp change(%{
         "event_type" => "deployment.rolled_back.v1",
         "payload" => %{"from_version" => from, "to_version" => to}
       }),
       do:
         if(version?(from) and version?(to),
           do: %{"kind" => "rolled_back", "from_version" => from, "to_version" => to},
           else: %{"kind" => "recorded"}
         )

  defp change(_), do: %{"kind" => "recorded"}

  defp version?(v),
    do: is_binary(v) and byte_size(v) in 1..48 and Regex.match?(~r/\A[A-Za-z0-9._-]+\z/, v)

  defp at_snapshot?(%{"timestamp" => time}, point) do
    case instant(time) do
      {:ok, _, timestamp} -> timestamp <= point
      _ -> false
    end
  end

  defp at_snapshot?(_, _), do: false

  defp covers_end?(page) do
    {:ok, _, point} = instant(page["read_point"])
    {:ok, _, last} = instant(page["until"])
    point >= last
  end

  defp same_instant?(nil, nil), do: true

  defp same_instant?(x, y) do
    with {:ok, _, a} <- instant(x),
         {:ok, _, b} <- instant(y),
         do: a == b,
         else: (_ -> false)
  end

  # Core stores nanoseconds. DateTime drops digits after microseconds.
  defp instant(v) when is_binary(v) and byte_size(v) in 20..40 do
    with [_, sec, frac, zone] <-
           Regex.run(
             ~r/\A(\d{4}-\d\d-\d\d[Tt]\d\d:\d\d:\d\d)(\.\d{1,9}|)([Zz]|[+-]\d\d:\d\d)\z/,
             v
           ),
         {:ok, date, _} <- DateTime.from_iso8601(String.upcase(sec <> zone)) do
      digits = String.trim_leading(frac, ".")

      nanos =
        DateTime.to_unix(date, :second) * 1_000_000_000 +
          String.to_integer(String.pad_trailing(digits, 9, "0"))

      utc = String.trim_trailing(DateTime.to_iso8601(date), "Z")
      {:ok, utc <> frac <> "Z", nanos}
    else
      _ -> :error
    end
  end

  defp instant(_), do: :error
end
