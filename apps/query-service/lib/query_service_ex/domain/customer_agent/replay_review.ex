defmodule QueryServiceEx.Domain.CustomerAgent.ReplayReview do
  @moduledoc "Exact rebuild plan and minimal decision receipt. Application services must establish human authority."
  alias QueryServiceEx.Domain.AgentRun.Event
  alias QueryServiceEx.Domain.CustomerAgent.EvidenceSource
  alias QueryServiceEx.Domain.CustomerAgent.Proposal
  alias QueryServiceEx.Domain.CustomerAgent.ReplaySource
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOperation, as: Operation
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOwner, as: Owner

  @keys ~w(schema_version id tenant_id subject_id client_id resource grant_id version proposal snapshot action authority_version created_at updated_at expires_at state digest request_sha256 replay_operation_id decision)
  @authority "allsource-operator-review-v1"
  @action "start_tenant_projection_rebuild"

  def new(owner, proposal, source, operation, now) do
    with true <- Owner.valid?(owner) and Operation.valid_at?(operation, now),
         :ok <- source_matches(owner, proposal, source, now) do
      expires =
        Enum.min([
          source["expires_at"],
          owner["grant_expires_at"],
          owner["entitlement_expires_at"] || source["expires_at"]
        ])

      record =
        Owner.take(owner)
        |> Map.merge(%{
          "schema_version" => 1,
          "id" => Owner.object_id(owner, "replay-review", operation),
          "version" => 1,
          "proposal" => proposal,
          "snapshot" => source["snapshot"],
          "action" => action(proposal["projection_name"]),
          "authority_version" => @authority,
          "created_at" => now,
          "updated_at" => now,
          "expires_at" => expires,
          "state" => "pending",
          "request_sha256" => Owner.digest(proposal),
          "replay_operation_id" => operation_id(),
          "decision" => nil
        })

      {:ok, Map.put(record, "digest", digest(record))}
    else
      _ -> {:error, :invalid_review}
    end
  end

  def valid?(record, tenant) when is_map(record) do
    Enum.sort(Map.keys(record)) == Enum.sort(@keys) and Owner.valid?(record) and
      record["tenant_id"] == tenant and
      valid_identity?(record) and valid_plan?(record) and valid_time?(record) and
      valid_decision?(record) and
      Owner.hash?(record["request_sha256"]) and record["digest"] == digest(record)
  end

  def valid?(_, _), do: false

  def revise(record, owner, proposal, source, operation, now) do
    with :ok <- source_matches(owner, proposal, source, now),
         true <- record["state"] == "pending" and Owner.matches?(record, owner),
         true <- Operation.valid_at?(operation, now) and now < record["expires_at"] do
      next = %{
        record
        | "version" => record["version"] + 1,
          "proposal" => proposal,
          "snapshot" => source["snapshot"],
          "action" => action(proposal["projection_name"]),
          "updated_at" => now,
          "expires_at" => min(record["expires_at"], source["expires_at"]),
          "request_sha256" =>
            Owner.digest([record["digest"], record["version"], proposal, operation])
      }

      {:ok, Map.put(next, "digest", digest(next))}
    else
      _ -> {:error, :review_conflict}
    end
  end

  def decide(record, actor, decision, operation, now) do
    with true <-
           record["state"] == "pending" and now >= record["updated_at"] and
             now < record["expires_at"],
         true <-
           actor["subject_id"] == record["subject_id"] and
             actor["tenant_id"] == record["tenant_id"],
         true <- actor["membership_role"] == "admin" and decision in ~w(approved rejected),
         true <- Event.uuid?(operation) do
      receipt = %{
        "id" => operation,
        "actor" => actor["subject_id"],
        "role" => "admin",
        "version" => record["version"],
        "digest" => record["digest"],
        "at" => now,
        "expires_at" => record["expires_at"],
        "operation" => @action,
        "replay_operation_id" => record["replay_operation_id"]
      }

      {:ok, %{record | "state" => decision, "decision" => receipt}}
    else
      _ -> {:error, :review_conflict}
    end
  end

  def digest(record),
    do:
      Owner.digest([
        "rebuild-plan-v1",
        Map.drop(record, ~w(digest state decision request_sha256 updated_at))
      ])

  def reference(record),
    do:
      record
      |> Map.take(~w(id version digest state expires_at replay_operation_id))
      |> Map.merge(%{
        "schema_version" => 1,
        "view_schema" => "rebuild-plan-v1",
        "human_approval" => "required_in_product"
      })

  defp action(projection),
    do: %{
      "operation" => @action,
      "projection_name" => projection,
      "history" => "retained_at_dispatch",
      "live_catchup" => true
    }

  defp source_matches(owner, proposal, source, now) do
    with {:ok, %{kind: "replay_plan", sources: [reference], projection_name: target}} <-
           Proposal.validate(proposal),
         true <- ReplaySource.valid?(source, owner["tenant_id"]) and source["locator"] == target,
         :ok <- EvidenceSource.authorize(source, owner, reference, now) do
      :ok
    else
      _ -> {:error, :invalid_review}
    end
  end

  defp valid_identity?(record),
    do:
      record["schema_version"] === 1 and Owner.id?(record["id"]) and
        record["authority_version"] == @authority and Event.uuid?(record["replay_operation_id"]) and
        is_integer(record["version"]) and record["version"] in 1..1_000

  defp valid_plan?(record) do
    with {:ok,
          %{
            kind: "replay_plan",
            sources: [%{"kind" => "replay_analysis"} = source],
            projection_name: target
          }} <- Proposal.validate(record["proposal"]),
         true <- ReplaySource.valid_snapshot?(record["snapshot"]) do
      source["sha256"] == Owner.digest(record["snapshot"]) and source["revision"] === 1 and
        target == record["snapshot"]["analysis"]["projection_name"] and
        record["action"] == action(target)
    else
      _ -> false
    end
  end

  defp valid_time?(record),
    do:
      Enum.all?(~w(created_at updated_at expires_at), &Owner.timestamp?(record[&1])) and
        record["created_at"] <= record["updated_at"] and
        record["updated_at"] < record["expires_at"] and
        record["expires_at"] - record["created_at"] <= 86_400

  defp valid_decision?(%{"state" => "pending", "decision" => nil}), do: true

  defp valid_decision?(%{"state" => state, "decision" => receipt} = record)
       when state in ["approved", "rejected"] and is_map(receipt) do
    Enum.sort(Map.keys(receipt)) ==
      ~w(actor at digest expires_at id operation replay_operation_id role version) and
      Event.uuid?(receipt["id"]) and receipt["actor"] == record["subject_id"] and
      receipt["role"] == "admin" and receipt_binding?(receipt, record) and
      receipt_time?(receipt, record)
  end

  defp valid_decision?(_), do: false

  defp receipt_binding?(receipt, record),
    do:
      receipt["version"] == record["version"] and receipt["digest"] == record["digest"] and
        receipt["expires_at"] == record["expires_at"] and receipt["operation"] == @action and
        receipt["replay_operation_id"] == record["replay_operation_id"]

  defp receipt_time?(receipt, record),
    do:
      Owner.timestamp?(receipt["at"]) and
        receipt["at"] >= record["updated_at"] and receipt["at"] < record["expires_at"]

  defp operation_id do
    <<a::32, b::16, _::4, c::12, _::2, d::14, e::48>> = :crypto.strong_rand_bytes(16)
    value = Base.encode16(<<a::32, b::16, 4::4, c::12, 2::2, d::14, e::48>>, case: :lower)

    <<p::binary-size(8), q::binary-size(4), r::binary-size(4), s::binary-size(4), t::binary>> =
      value

    "#{p}-#{q}-#{r}-#{s}-#{t}"
  end
end
