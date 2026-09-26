defmodule QueryServiceEx.Domain.CustomerAgent.Proposal do
  @moduledoc """
  Bounded customer investigation requests, without credentials or action authority.

  Existing query entities accept arbitrary predicates; this separate domain type
  deliberately accepts only opaque evidence references. Validation establishes
  syntax, not source ownership or truth. Resolution belongs to the tenant service.
  For comparisons, the first run reference is the baseline and the second is the
  candidate. Source order is therefore part of the proposal fingerprint.
  """

  @enforce_keys [:kind, :projection_name, :sources]
  defstruct [:kind, :projection_name, :sources]

  @type source :: %{
          String.t() => String.t() | pos_integer()
        }
  @type t :: %__MODULE__{kind: String.t(), projection_name: String.t() | nil, sources: [source()]}
  @type error :: {:error, atom()}

  @fields ~w(schema_version kind projection_name sources)
  @source_fields ~w(kind ref revision sha256)
  @kinds ~w(event_timeline run_comparison replay_plan)
  @source_kinds ~w(event_range run_evidence restart_evidence replay_analysis)
  @max_integer 9_007_199_254_740_991
  @max_bytes 65_536

  @doc "Decode a bounded JSON request. Errors never include customer values."
  @spec decode(term()) :: {:ok, t()} | error()
  def decode(json) when is_binary(json) and byte_size(json) > @max_bytes,
    do: {:error, :input_too_large}

  def decode(json) when is_binary(json) do
    case Jason.decode(json) do
      {:ok, input} -> validate(input)
      {:error, _} -> {:error, :invalid_json}
    end
  end

  def decode(_), do: {:error, :invalid_json}

  @doc "Validate decoded input; all reference authority remains unresolved."
  @spec validate(term()) :: {:ok, t()} | error()
  def validate(input) do
    with :ok <- shape(input, @fields, :invalid_shape),
         :ok <- version(input["schema_version"]),
         :ok <- kind(input["kind"]),
         :ok <- target(input["kind"], input["projection_name"]),
         :ok <- sources(input["sources"]),
         :ok <- compatible(input["kind"], input["sources"]) do
      {:ok,
       %__MODULE__{
         kind: input["kind"],
         projection_name: input["projection_name"],
         sources: input["sources"]
       }}
    end
  end

  @doc "Missing evidence and unresolved authority are explicit, even for valid syntax."
  @spec unknowns(t()) :: [String.t()]
  def unknowns(%__MODULE__{} = proposal) do
    missing =
      case {proposal.kind, count(proposal.sources, required_source(proposal.kind))} do
        {"event_timeline", 0} -> ["missing_event_sources"]
        {"run_comparison", n} when n < 2 -> ["missing_comparison_source"]
        {"replay_plan", 0} -> ["missing_replay_analysis"]
        _ -> []
      end

    missing ++ ["unresolved_source_authority"]
  end

  @doc "Canonical request fingerprint, not an approval or signature."
  @spec fingerprint(t()) :: String.t()
  def fingerprint(%__MODULE__{} = proposal) do
    sources = Enum.map(proposal.sources, &Enum.map(@source_fields, fn key -> &1[key] end))

    bytes =
      Jason.encode!(["allsource-proposal-v1", proposal.kind, proposal.projection_name, sources])

    :sha256 |> :crypto.hash(bytes) |> Base.encode16(case: :lower)
  end

  defp shape(value, keys, error) when is_map(value) do
    if Enum.sort(Map.keys(value)) == Enum.sort(keys), do: :ok, else: {:error, error}
  end

  defp shape(_, _, error), do: {:error, error}
  defp version(1), do: :ok
  defp version(_), do: {:error, :unsupported_version}
  defp kind(value) when value in @kinds, do: :ok
  defp kind(_), do: {:error, :invalid_kind}

  defp target("replay_plan", name) when is_binary(name) and byte_size(name) in 1..64 do
    if Regex.match?(~r/\A[a-z][a-z0-9-]*\z/, name), do: :ok, else: {:error, :invalid_target}
  end

  defp target(kind, nil) when kind in ["event_timeline", "run_comparison"], do: :ok
  defp target(_, _), do: {:error, :invalid_target}

  defp sources(values) when is_list(values) and length(values) <= 32 do
    cond do
      not Enum.all?(values, &valid_source?/1) -> {:error, :invalid_source}
      length(Enum.uniq_by(values, & &1["ref"])) != length(values) -> {:error, :duplicate_source}
      true -> :ok
    end
  end

  defp sources(_), do: {:error, :invalid_source_count}

  defp valid_source?(source) do
    shape(source, @source_fields, :invalid_source) == :ok and
      source["kind"] in @source_kinds and
      valid_ref?(source["ref"]) and valid_revision?(source["revision"]) and
      valid_digest?(source["sha256"])
  end

  defp valid_ref?(value) when is_binary(value) and byte_size(value) in 16..128,
    do: Regex.match?(~r/\A[A-Za-z0-9_-]+\z/, value)

  defp valid_ref?(_), do: false
  defp valid_revision?(value), do: is_integer(value) and value in 1..@max_integer

  defp valid_digest?(value) when is_binary(value) and byte_size(value) == 64,
    do: Regex.match?(~r/\A[0-9a-f]{64}\z/, value)

  defp valid_digest?(_), do: false

  defp compatible(kind, sources) do
    allowed = allowed_sources(kind)

    if Enum.all?(sources, &(&1["kind"] in allowed)) and within_kind_limit?(kind, sources),
      do: :ok,
      else: {:error, :incompatible_sources}
  end

  defp allowed_sources("event_timeline"), do: ~w(event_range restart_evidence)
  defp allowed_sources("run_comparison"), do: ~w(run_evidence restart_evidence)
  defp allowed_sources("replay_plan"), do: ~w(replay_analysis event_range restart_evidence)
  defp within_kind_limit?("run_comparison", sources), do: count(sources, "run_evidence") <= 2
  defp within_kind_limit?("replay_plan", sources), do: count(sources, "replay_analysis") <= 1
  defp within_kind_limit?(_, _), do: true
  defp count(sources, kind), do: Enum.count(sources, &(&1["kind"] == kind))
  defp required_source("event_timeline"), do: "event_range"
  defp required_source("run_comparison"), do: "run_evidence"
  defp required_source("replay_plan"), do: "replay_analysis"
end
