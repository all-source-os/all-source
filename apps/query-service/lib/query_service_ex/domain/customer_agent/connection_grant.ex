defmodule QueryServiceEx.Domain.CustomerAgent.ConnectionGrant do
  @moduledoc """
  Narrow agent credential bindings, distinct from user sessions and generic JWTs.

  These checks establish a connection credential only. They do not establish live
  membership, source ownership, processing consent, paid access or human approval.
  No existing API-key type provides this restricted operation vocabulary.
  """

  @binding_keys ~w(tenant_id subject_id client_id resource)
  @operations ~w(read_context validate_proposal prepare_proposal read_review read_result)
  @version "allsource-customer-grant-v1"

  @spec new(map(), list(), integer(), integer()) :: {:ok, map()} | {:error, :invalid_grant}
  def new(binding, operations, now, ttl) do
    if valid_binding?(binding) and valid_operations?(operations) and
         is_integer(now) and now >= 0 and is_integer(ttl) and ttl in 1..86_400 do
      {:ok,
       Map.merge(binding, %{
         "version" => @version,
         "operations" => operations,
         "created_at" => now,
         "expires_at" => now + ttl,
         "active" => true
       })}
    else
      {:error, :invalid_grant}
    end
  end

  @spec valid_binding?(term()) :: boolean()
  def valid_binding?(binding) when is_map(binding) do
    Enum.sort(Map.keys(binding)) == Enum.sort(@binding_keys) and
      Enum.all?(~w(tenant_id subject_id client_id), &valid_id?(binding[&1])) and
      valid_resource?(binding["resource"])
  end

  def valid_binding?(_), do: false

  @spec valid_id?(term()) :: boolean()
  def valid_id?(value) when is_binary(value) and byte_size(value) in 1..128,
    do: Regex.match?(~r/\A[A-Za-z0-9_-]+\z/, value)

  def valid_id?(_), do: false

  @spec matches_owner?(term(), term()) :: boolean()
  def matches_owner?(record, binding) when is_map(record) do
    valid_binding?(binding) and record["version"] == @version and
      Enum.all?(@binding_keys, &(record[&1] == binding[&1]))
  end

  def matches_owner?(_, _), do: false

  @spec valid_for?(term(), term(), term(), term()) :: boolean()
  def valid_for?(record, binding, operation, now) when is_map(record) do
    matches_owner?(record, binding) and record["active"] == true and
      valid_operations?(record["operations"]) and operation in record["operations"] and
      valid_interval?(record, now)
  end

  def valid_for?(_, _, _, _), do: false

  defp valid_operations?(operations) when is_list(operations) and length(operations) in 1..5,
    do:
      Enum.all?(operations, &(&1 in @operations)) and
        length(Enum.uniq(operations)) == length(operations)

  defp valid_operations?(_), do: false

  defp valid_interval?(record, now) do
    created = record["created_at"]
    expires = record["expires_at"]

    is_integer(now) and is_integer(created) and created >= 0 and is_integer(expires) and
      (expires - created) in 1..86_400 and now >= created and now < expires
  end

  defp valid_resource?(value) when is_binary(value) and byte_size(value) in 1..512 do
    case URI.new(value) do
      {:ok, %URI{scheme: "https", host: host, userinfo: nil, query: nil, fragment: nil}}
      when is_binary(host) and byte_size(host) > 0 ->
        true

      _ ->
        false
    end
  end

  defp valid_resource?(_), do: false
end
