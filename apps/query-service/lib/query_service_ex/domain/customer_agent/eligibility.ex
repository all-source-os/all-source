defmodule QueryServiceEx.Domain.CustomerAgent.Eligibility do
  @moduledoc """
  Interpret current Core tenant and Control Plane team records for MCP review.

  Existing cached TenantContext permits missing billing fields; that is not
  entitlement evidence here. This policy reads the persisted MCP scope, never
  derives a new price/tier or grants access from a token role. Scope describes
  read-only review preparation, not ingestion or human approval.
  """

  alias QueryServiceEx.Domain.CustomerAgent.ConnectionGrant

  @scopes ~w(read read+write read+write+dedicated dedicated)
  @statuses ~w(active on_trial trialing past_due)
  @max_integer 9_007_199_254_740_991

  @spec check(term(), term(), term(), term()) :: {:ok, map()} | {:error, :access_denied}
  def check(tenant, members, binding, now), do: check(tenant, members, binding, now, :available)

  @doc "Current access without claiming a query unit; the caller must obtain atomic Core admission before source work."
  def check_metered(tenant, members, binding, now),
    do: check(tenant, members, binding, now, :admission_required)

  defp check(tenant, members, binding, now, budget) do
    with true <- ConnectionGrant.valid_binding?(binding),
         true <- is_integer(now) and now >= 0,
         true <- active_tenant?(tenant, binding["tenant_id"]),
         {:ok, role} <- member_role(members, binding["subject_id"]),
         {:ok, entitlement} <- entitlement(tenant["metadata"], now, budget) do
      {:ok, Map.put(entitlement, "membership_role", role)}
    else
      _ -> {:error, :access_denied}
    end
  end

  # This is the actual /api/v1/tenants/:id response shape, not the unused DTO
  # whose id is a generated UUID and whose active flag is named is_active.
  defp active_tenant?(%{"id" => id, "active" => true, "is_demo" => false}, expected),
    do: id == expected

  defp active_tenant?(_, _), do: false

  defp member_role(members, subject) when is_list(members) and length(members) <= 1_000 do
    if Enum.all?(members, &is_map/1) do
      case Enum.filter(members, &(&1["user_id"] == subject)) do
        [%{"role" => role}] when role in ["admin", "member"] -> {:ok, role}
        _ -> {:error, :access_denied}
      end
    else
      {:error, :access_denied}
    end
  end

  defp member_role(_, _), do: {:error, :access_denied}

  defp entitlement(%{"subscription" => sub, "quotas" => quotas} = metadata, now, budget)
       when is_map(sub) and is_map(quotas) do
    with true <- active_status?(sub["status"]),
         true <- quotas["mcp_scope"] in @scopes,
         {:ok, deadline} <- deadline(sub, metadata, now),
         {:ok, remaining} <- query_budget(quotas, budget) do
      {:ok,
       %{
         "mcp_scope" => quotas["mcp_scope"],
         "entitlement_expires_at" => deadline,
         "queries_remaining" => remaining
       }}
    end
  end

  defp entitlement(_, _, _), do: {:error, :access_denied}

  # Mirrors Control Plane SubscriptionIsActive, including its dunning grace.
  defp active_status?(status) when is_binary(status), do: String.downcase(status) in @statuses
  defp active_status?(_), do: false

  defp deadline(sub, metadata, now) do
    trial = sub["tier"] == "trial" or String.downcase(sub["status"]) in ~w(on_trial trialing)

    trial_dates = [sub["trial_expires_at"], sub["trial_ends_at"], metadata["trial_expires_at"]]
    # Historical trial dates on a converted paid subscription are not an expiry
    # of the new paid entitlement. SubscriptionEndsAt remains a hard boundary.
    dates = if trial, do: trial_dates, else: []
    dates = Enum.reject([sub["subscription_ends_at"] | dates], &is_nil/1)

    if trial and Enum.all?(trial_dates, &is_nil/1) do
      {:error, :access_denied}
    else
      parse_deadlines(dates, now)
    end
  end

  defp parse_deadlines(dates, now) do
    Enum.reduce_while(dates, {:ok, nil}, fn date, {:ok, earliest} ->
      case timestamp(date) do
        {:ok, expiry} when expiry > now ->
          {:cont, {:ok, if(is_nil(earliest), do: expiry, else: min(expiry, earliest))}}

        _ ->
          {:halt, {:error, :access_denied}}
      end
    end)
  end

  defp timestamp(value) when is_binary(value) and byte_size(value) in 1..40 do
    case DateTime.from_iso8601(value) do
      {:ok, date, _offset} -> {:ok, DateTime.to_unix(date)}
      _ -> {:error, :access_denied}
    end
  end

  defp timestamp(_), do: {:error, :access_denied}

  defp query_budget(%{"queries_quota" => -1}, :available), do: {:ok, -1}

  defp query_budget(%{"queries_quota" => limit, "queries_used" => used}, :admission_required)
       when is_integer(limit) and limit in -1..@max_integer and is_integer(used) and
              used in 0..@max_integer,
       do: {:ok, if(limit == -1, do: -1, else: max(0, limit - used))}

  defp query_budget(%{"queries_quota" => limit, "queries_used" => used}, :available)
       when is_integer(limit) and limit in 1..@max_integer and is_integer(used) and
              used >= 0 and used < limit,
       do: {:ok, limit - used}

  defp query_budget(_, _), do: {:error, :access_denied}
end
