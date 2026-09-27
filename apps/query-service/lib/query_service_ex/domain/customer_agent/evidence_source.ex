defmodule QueryServiceEx.Domain.CustomerAgent.EvidenceSource do
  @moduledoc "Expiring, owner/host/grant-bound references to a pinned typed run; no raw event payloads."
  alias QueryServiceEx.Domain.AgentRun.Event
  alias QueryServiceEx.Domain.CustomerAgent.ReplaySource
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOperation, as: Operation
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOwner, as: Owner

  @keys ~w(schema_version id tenant_id subject_id client_id resource grant_id kind locator revision sha256 request_sha256 created_at expires_at)

  def new(owner, run, operation, now, ttl) when is_map(owner) and is_map(run) do
    expiry = owner["grant_expires_at"]

    if Owner.valid?(owner) and Operation.valid_at?(operation, now) and valid_run?(run) and
         valid_lifetime?(now, ttl, expiry) do
      {:ok,
       Owner.take(owner)
       |> Map.merge(%{
         "schema_version" => 1,
         "id" => Owner.object_id(owner, "source", operation),
         "kind" => "run_evidence",
         "locator" => run.run_id,
         "revision" => run.revision,
         "sha256" => run.digest,
         "request_sha256" => Owner.digest([run.run_id, run.revision, run.digest, ttl]),
         "created_at" => now,
         "expires_at" => min(now + ttl, expiry)
       })}
    else
      {:error, :invalid_source}
    end
  end

  def new(_, _, _, _, _), do: {:error, :invalid_source}

  def valid?(%{"kind" => "replay_analysis"} = source, tenant),
    do: ReplaySource.valid?(source, tenant)

  def valid?(source, tenant) when is_map(source) do
    Enum.sort(Map.keys(source)) == Enum.sort(@keys) and source["schema_version"] === 1 and
      Owner.valid?(source) and source["tenant_id"] == tenant and Owner.id?(source["id"]) and
      source["kind"] == "run_evidence" and
      valid_run?(%{
        run_id: source["locator"],
        revision: source["revision"],
        digest: source["sha256"]
      }) and
      Owner.hash?(source["request_sha256"]) and valid_interval?(source)
  end

  def valid?(_, _), do: false

  defp valid_run?(run),
    do:
      Event.uuid?(run[:run_id]) and is_integer(run[:revision]) and
        run[:revision] in 1..1_000 and Owner.hash?(run[:digest])

  defp valid_lifetime?(now, ttl, expiry),
    do:
      Owner.timestamp?(now) and is_integer(ttl) and
        ttl in 1..3_600 and Owner.timestamp?(expiry) and expiry > now

  defp valid_interval?(source),
    do:
      Owner.timestamp?(source["created_at"]) and
        Owner.timestamp?(source["expires_at"]) and
        (source["expires_at"] - source["created_at"]) in 1..3_600

  def reference(source),
    do: %{
      "kind" => source["kind"],
      "ref" => source["id"],
      "revision" => source["revision"],
      "sha256" => source["sha256"]
    }

  def authorize(source, owner, reference, now) do
    cond do
      not is_map(owner) or not valid?(source, owner["tenant_id"]) or
          not Owner.matches?(source, owner) ->
        {:error, :source_denied}

      not Owner.timestamp?(now) or now < source["created_at"] ->
        {:error, :source_denied}

      now >= source["expires_at"] ->
        {:error, :source_expired}

      reference != reference(source) ->
        {:error, :source_changed}

      true ->
        :ok
    end
  end
end
