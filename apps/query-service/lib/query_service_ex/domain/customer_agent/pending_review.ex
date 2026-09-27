defmodule QueryServiceEx.Domain.CustomerAgent.PendingReview do
  @moduledoc "Minimal pending review metadata. No approval or execution state can be constructed here."
  alias QueryServiceEx.Domain.CustomerAgent.Proposal
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOperation, as: Operation
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOwner, as: Owner

  @keys ~w(schema_version id tenant_id subject_id client_id resource grant_id version state proposal request_sha256 digest view_schema authority_version created_at expires_at)
  @view "run-comparison-v1"
  @authority "allsource-operator-review-v1"

  def new(owner, proposal, report, operation, now, expires_at) do
    with true <- Owner.valid?(owner) and Operation.valid_at?(operation, now),
         true <- Owner.timestamp?(now) and Owner.timestamp?(expires_at),
         true <- (expires_at - now) in 1..86_400,
         true <-
           Owner.timestamp?(owner["grant_expires_at"]) and expires_at <= owner["grant_expires_at"],
         {:ok, typed} <- Proposal.validate(proposal),
         true <- typed.kind == "run_comparison" do
      record =
        Owner.take(owner)
        |> Map.merge(%{
          "schema_version" => 1,
          "id" => Owner.object_id(owner, "review", operation),
          "version" => 1,
          "state" => "pending",
          "proposal" => proposal,
          "request_sha256" => Proposal.fingerprint(typed),
          "view_schema" => @view,
          "authority_version" => @authority,
          "created_at" => now,
          "expires_at" => expires_at
        })

      {:ok, Map.put(record, "digest", digest(record, report))}
    else
      _ -> {:error, :invalid_review}
    end
  end

  def valid?(record, tenant) when is_map(record) do
    Enum.sort(Map.keys(record)) == Enum.sort(@keys) and valid_contract?(record) and
      Owner.valid?(record) and record["tenant_id"] == tenant and Owner.id?(record["id"]) and
      Owner.hash?(record["digest"]) and valid_proposal?(record) and valid_interval?(record)
  end

  def valid?(_, _), do: false

  defp valid_contract?(record),
    do:
      record["schema_version"] === 1 and record["version"] === 1 and
        record["state"] == "pending" and record["view_schema"] == @view and
        record["authority_version"] == @authority

  defp valid_interval?(record),
    do:
      Owner.timestamp?(record["created_at"]) and Owner.timestamp?(record["expires_at"]) and
        (record["expires_at"] - record["created_at"]) in 1..86_400

  def digest(record, report),
    do: Owner.digest(["customer-review-v1", Map.drop(record, ["digest"]), report])

  defp valid_proposal?(record) do
    case Proposal.validate(record["proposal"]) do
      {:ok, %{kind: "run_comparison"} = proposal} ->
        Proposal.fingerprint(proposal) == record["request_sha256"]

      _ ->
        false
    end
  end
end
