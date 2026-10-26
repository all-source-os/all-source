defmodule QueryServiceEx.Views.Reducer do
  @moduledoc """
  Pure fold of one event into one row of a declarative view.

  Implements the definition language of the latest-state contract
  (`docs/contracts/latest-state/README.md`, section 4): the row-key template,
  `on`/`when` matching, value expressions and the operations `set`, `unset`,
  `delete`, `increment`, `max`, `min`, `append`, `remove` and `status`.

  Definitions arrive as decoded JSON (string keys) and are validated by Core
  before they are stored, so an expression this module does not recognise is
  treated as a missing value rather than raised.
  """

  import Kernel, except: [apply: 3]

  @max_key_bytes 512
  @operations ~w(set unset delete increment max min append remove status)
  @event_atoms %{
    "entity_id" => :entity_id,
    "event_type" => :event_type,
    "payload" => :payload,
    "timestamp" => :timestamp
  }

  @type definition :: map()
  @type row :: map() | nil
  @type event :: map()
  @type result :: {:upsert, String.t(), map()} | {:delete, String.t()} | {:skip, atom()}
  @type stats :: %{rejected_transition: non_neg_integer()}

  @doc """
  Fold `event` into `row` (the current row for the event's key, or `nil`).
  """
  @spec apply(definition(), row(), event()) :: result()
  def apply(definition, row, event) do
    {result, _stats} = fold(definition, row, event)
    result
  end

  @doc """
  Same as `apply/3`, also returning how many status transitions were rejected.
  """
  @spec fold(definition(), row(), event()) :: {result(), stats()}
  def fold(definition, row, event) do
    case prepare(definition, event) do
      {:ok, key, ops} ->
        {next, rejected} = Enum.reduce(ops, {row, 0}, &apply_op(&1, &2, event))
        {finish(key, next), %{rejected_transition: rejected}}

      {:skip, reason} ->
        {{:skip, reason}, %{rejected_transition: 0}}
    end
  end

  @doc """
  The row key `event` folds into, without touching any row.

  Lets a caller load the current row before calling `apply/3`.
  """
  @spec key(definition(), event()) :: {:ok, String.t()} | {:skip, atom()}
  def key(definition, event) do
    case prepare(definition, event) do
      {:ok, key, _ops} -> {:ok, key}
      {:skip, reason} -> {:skip, reason}
    end
  end

  defp prepare(definition, event) do
    event_type = field(event, "event_type")

    cond do
      not is_binary(event_type) ->
        {:skip, :invalid_event}

      not in_source?(definition, event_type) ->
        {:skip, :outside_source}

      true ->
        case matching_ops(definition, event_type, event) do
          [] ->
            {:skip, :no_matching_op}

          ops ->
            with {:ok, key} <- render_key(Map.get(definition, "key"), event) do
              {:ok, key, ops}
            end
        end
    end
  end

  defp in_source?(definition, event_type) do
    case get_in(definition, ["source", "prefixes"]) do
      prefixes when is_list(prefixes) and prefixes != [] ->
        Enum.any?(prefixes, &(is_binary(&1) and String.starts_with?(event_type, &1)))

      _ ->
        true
    end
  end

  defp matching_ops(definition, event_type, event) do
    definition
    |> Map.get("ops", [])
    |> List.wrap()
    |> Enum.filter(&matches?(&1, event_type, event))
  end

  defp matches?(op, event_type, event) when is_map(op) do
    on?(op["on"], event_type) and conditions_met?(op["when"], event)
  end

  defp matches?(_op, _event_type, _event), do: false

  defp on?(pattern, event_type) when is_binary(pattern) do
    cond do
      String.ends_with?(pattern, "*") ->
        String.starts_with?(event_type, String.trim_trailing(pattern, "*"))

      String.ends_with?(pattern, ".") ->
        String.starts_with?(event_type, pattern)

      true ->
        pattern == event_type
    end
  end

  defp on?(_pattern, _event_type), do: false

  defp conditions_met?(nil, _event), do: true

  defp conditions_met?(conditions, event) when is_map(conditions) do
    Enum.all?(conditions, fn {path, expected} ->
      case resolve(path, event) do
        {:ok, value} -> value == expected
        :missing -> is_nil(expected)
      end
    end)
  end

  defp conditions_met?(_conditions, _event), do: false

  ## Key template

  defp render_key(template, event) when is_binary(template) do
    ~r/\{([^}]*)\}/
    |> Regex.split(template, include_captures: true)
    |> Enum.reduce_while({:ok, ""}, fn part, {:ok, acc} ->
      case key_segment(part, event) do
        {:ok, text} -> {:cont, {:ok, acc <> text}}
        :missing -> {:halt, {:skip, :missing_key_field}}
      end
    end)
    |> check_key()
  end

  defp render_key(_template, _event), do: {:skip, :invalid_key}

  defp key_segment("{" <> rest, event) do
    expression = String.trim_trailing(rest, "}")

    case resolve(expression, event) do
      {:ok, value} -> scalar_text(value)
      :missing -> :missing
    end
  end

  defp key_segment(literal, _event), do: {:ok, literal}

  defp scalar_text(value) when is_binary(value), do: {:ok, value}
  defp scalar_text(value) when is_integer(value) or is_float(value), do: {:ok, to_string(value)}
  defp scalar_text(value) when is_boolean(value), do: {:ok, to_string(value)}
  defp scalar_text(_value), do: :missing

  defp check_key({:ok, ""}), do: {:skip, :empty_key}
  defp check_key({:ok, key}) when byte_size(key) > @max_key_bytes, do: {:skip, :key_too_long}
  defp check_key(other), do: other

  ## Value expressions

  defp value(%{"literal" => literal}, _event), do: {:ok, literal}
  defp value(expression, event) when is_binary(expression), do: resolve(expression, event)
  defp value(_expression, _event), do: :missing

  defp resolve("payload." <> path, event) do
    dig(field(event, "payload"), String.split(path, "."))
  end

  defp resolve("day:timestamp", event) do
    with value when is_binary(value) <- field(event, "timestamp"),
         {:ok, datetime, _offset} <- DateTime.from_iso8601(value) do
      {:ok, datetime |> DateTime.to_date() |> Date.to_iso8601()}
    else
      _ -> :missing
    end
  end

  defp resolve(name, event) when name in ["entity_id", "event_type", "timestamp"] do
    case field(event, name) do
      nil -> :missing
      value -> {:ok, value}
    end
  end

  defp resolve(_expression, _event), do: :missing

  defp dig(value, []), do: {:ok, value}

  defp dig(map, [segment | rest]) when is_map(map) do
    case Map.fetch(map, segment) do
      {:ok, value} -> dig(value, rest)
      :error -> :missing
    end
  end

  defp dig(_value, _path), do: :missing

  defp field(event, name) do
    case Map.fetch(event, name) do
      {:ok, value} -> value
      :error -> Map.get(event, Map.fetch!(@event_atoms, name))
    end
  end

  ## Operations

  defp apply_op(op, acc, event) do
    op
    |> Map.take(@operations)
    |> Enum.sort_by(fn {name, _arg} -> Enum.find_index(@operations, &(&1 == name)) end)
    |> Enum.reduce(acc, fn {name, arg}, {row, rejected} ->
      operate(name, arg, row, rejected, event)
    end)
  end

  defp operate("delete", true, _row, rejected, _event), do: {nil, rejected}

  defp operate("unset", _fields, nil, rejected, _event), do: {nil, rejected}

  defp operate("unset", fields, row, rejected, _event) when is_list(fields) do
    {Map.drop(row, fields), rejected}
  end

  defp operate("set", fields, row, rejected, event) when is_map(fields) do
    {assign(row, fields, event, fn _current, new -> {:ok, new} end), rejected}
  end

  defp operate("increment", fields, row, rejected, _event) when is_map(fields) do
    next =
      Enum.reduce(fields, row || %{}, fn
        {name, by}, acc when is_integer(by) ->
          current = Map.get(acc, name)
          Map.put(acc, name, if(is_integer(current), do: current, else: 0) + by)

        _field, acc ->
          acc
      end)

    {next, rejected}
  end

  defp operate("max", fields, row, rejected, event) when is_map(fields) do
    {assign(row, fields, event, &keep(&1, &2, :gt)), rejected}
  end

  defp operate("min", fields, row, rejected, event) when is_map(fields) do
    {assign(row, fields, event, &keep(&1, &2, :lt)), rejected}
  end

  defp operate("append", fields, row, rejected, event) when is_map(fields) do
    next =
      assign(row, fields, event, fn current, new ->
        list = if is_list(current), do: current, else: []
        if new in list, do: :keep, else: {:ok, list ++ [new]}
      end)

    {next, rejected}
  end

  defp operate("remove", fields, row, rejected, event) when is_map(fields) do
    next =
      assign(row, fields, event, fn
        current, new when is_list(current) ->
          if new in current, do: {:ok, List.delete(current, new)}, else: :keep

        _current, _new ->
          :keep
      end)

    {next, rejected}
  end

  defp operate("status", %{"field" => name} = spec, row, rejected, event) when is_binary(name) do
    current_row = row || %{}
    current = Map.get(current_row, name)

    case status_value(Map.get(spec, "from"), event) do
      {:ok, ^current} ->
        {current_row, rejected}

      {:ok, new} ->
        if transition_allowed?(Map.get(spec, "transitions"), current, new),
          do: {Map.put(current_row, name, new), rejected},
          else: {row, rejected + 1}

      :missing ->
        {row, rejected}
    end
  end

  defp operate(_name, _arg, row, rejected, _event), do: {row, rejected}

  defp assign(row, fields, event, decide) do
    Enum.reduce(fields, row || %{}, fn {name, expression}, acc ->
      with {:ok, new} <- value(expression, event),
           {:ok, next} <- decide.(Map.get(acc, name), new) do
        Map.put(acc, name, next)
      else
        _ -> acc
      end
    end)
  end

  defp keep(nil, new, _direction), do: {:ok, new}

  defp keep(current, new, direction) do
    if comparable?(current, new) and compare(new, current) == direction,
      do: {:ok, new},
      else: :keep
  end

  defp comparable?(a, b) when is_number(a) and is_number(b), do: true
  defp comparable?(a, b) when is_binary(a) and is_binary(b), do: true
  defp comparable?(_a, _b), do: false

  defp compare(a, b) when a > b, do: :gt
  defp compare(a, b) when a < b, do: :lt
  defp compare(_a, _b), do: :eq

  defp status_value("suffix", event) do
    case field(event, "event_type") do
      type when is_binary(type) -> {:ok, type |> String.split(".") |> List.last()}
      _ -> :missing
    end
  end

  defp status_value(expression, event), do: value(expression, event)

  defp transition_allowed?(nil, _current, _new), do: true
  defp transition_allowed?(_transitions, nil, _new), do: true

  defp transition_allowed?(transitions, current, new) when is_map(transitions) do
    case Map.get(transitions, to_string_key(current)) do
      allowed when is_list(allowed) -> new in allowed
      _ -> false
    end
  end

  defp transition_allowed?(_transitions, _current, _new), do: false

  defp to_string_key(value) when is_binary(value), do: value
  defp to_string_key(value), do: Jason.encode!(value)

  defp finish(key, nil), do: {:delete, key}
  defp finish(key, row), do: {:upsert, key, row}
end
