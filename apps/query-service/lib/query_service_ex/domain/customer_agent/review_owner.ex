defmodule QueryServiceEx.Domain.CustomerAgent.ReviewOwner do
  @moduledoc "Shared owner binding and canonical fingerprints; neither authenticates a caller."
  alias QueryServiceEx.Domain.CustomerAgent.ConnectionGrant
  alias QueryServiceEx.Domain.CustomerAgent.ConnectionRegistry
  @fields ~w(tenant_id subject_id client_id resource grant_id)

  def fields, do: @fields
  def take(value), do: Map.take(value, @fields)

  def valid?(value) when is_map(value),
    do:
      ConnectionGrant.valid_binding?(Map.take(value, ~w(tenant_id subject_id client_id resource))) and
        ConnectionRegistry.valid_id?(value["grant_id"])

  def valid?(_), do: false
  def matches?(left, right), do: valid?(left) and valid?(right) and take(left) == take(right)
  def id?(id), do: ConnectionRegistry.valid_id?(id)

  def hash?(value) when is_binary(value) and byte_size(value) == 64,
    do: Regex.match?(~r/\A[0-9a-f]{64}\z/, value)

  def hash?(_), do: false
  def timestamp?(value), do: is_integer(value) and value >= 0 and value <= 9_007_199_254_740_991

  def digest(value),
    do: :sha256 |> :crypto.hash(Jason.encode!(canonical(value))) |> Base.encode16(case: :lower)

  def object_id(owner, kind, operation),
    do: digest(["customer-review-object-v1", take(owner), kind, operation]) |> binary_part(0, 32)

  defp canonical(value) when is_map(value),
    do:
      value
      |> Enum.map(fn {key, item} -> [to_string(key), canonical(item)] end)
      |> Enum.sort_by(&hd/1)

  defp canonical(value) when is_list(value), do: Enum.map(value, &canonical/1)
  defp canonical(value), do: value
end
