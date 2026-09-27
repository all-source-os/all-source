defmodule QueryServiceEx.Application.Services.CustomerReplayReview do
  @moduledoc "Product decisions over exact rebuild plans; agent entry points only prepare and read."
  alias QueryServiceEx.Application.Services.CustomerAgentAccess
  alias QueryServiceEx.Application.Services.CustomerQueryAdmission, as: Admission
  alias QueryServiceEx.Application.Services.CustomerReplaySources, as: Sources
  alias QueryServiceEx.Application.Services.CustomerReviewDeadline
  alias QueryServiceEx.Application.Services.CustomerReviewRecords, as: Records
  alias QueryServiceEx.Domain.AgentRun.Event
  alias QueryServiceEx.Domain.CustomerAgent.ConnectionConsent
  alias QueryServiceEx.Domain.CustomerAgent.EvidenceSource
  alias QueryServiceEx.Domain.CustomerAgent.Proposal
  alias QueryServiceEx.Domain.CustomerAgent.ReplayReview, as: Review
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOperation, as: Operation
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOwner, as: Owner
  alias QueryServiceEx.Infrastructure.Adapters.CustomerActionStore, as: Store
  alias QueryServiceEx.Projections.TrackedReplays

  def prepare(token, binding, input, now) do
    CustomerReviewDeadline.run(binding, fn ->
      do_prepare(fn time -> agent_owner(token, binding, "prepare_proposal", time) end, input, now)
    end)
  end

  def prepare_human(actor, connection, input, now) do
    CustomerReviewDeadline.run(actor, fn ->
      do_prepare(fn time -> Sources.owner(actor, connection, time) end, input, now)
    end)
  end

  def read_human(actor, connection, input, now) do
    CustomerReviewDeadline.run(actor, fn ->
      do_read(fn time -> Sources.owner(actor, connection, time, "read_review") end, input, now)
    end)
  end

  def read(token, binding, input, now, operation \\ "read_review") do
    CustomerReviewDeadline.run(binding, fn ->
      do_read(fn time -> agent_owner(token, binding, operation, time) end, input, now)
    end)
  end

  def edit(actor, connection, input, now) do
    CustomerReviewDeadline.run(actor, fn ->
      with true <- keys?(input, ~w(digest id idempotency_key proposal version)),
           true <- is_integer(input["version"]) and input["version"] in 1..999,
           true <- Operation.valid_at?(input["idempotency_key"], now),
           {:ok, owner} <- operator(actor, connection, now),
           {:ok, current} <- Store.fetch(owner["tenant_id"], input["id"]),
           true <- Owner.matches?(current, owner),
           {:ok, next} <- edit_current(owner, current, input, now),
           {:ok, _} <- operator(actor, connection, System.system_time(:second)) do
        if next["state"] == "pending",
          do: {:ok, display(next, "pending", nil)},
          else: result(next)
      else
        false -> {:error, :invalid_request}
        error -> error
      end
    end)
  end

  def decide(actor, connection, input, decision, now) when decision in ["approved", "rejected"] do
    CustomerReviewDeadline.run(actor, fn ->
      with true <- keys?(input, ~w(decision_id digest id request_id version)),
           true <-
             Event.uuid?(input["decision_id"]) and Operation.valid_at?(input["request_id"], now),
           {:ok, owner} <- operator(actor, connection, now),
           {:ok, current} <- owned(owner, input),
           :ok <- decision_retry(current, decision, input["decision_id"]),
           {:ok, mode} <- decision_freshness(owner, current, input, decision, now),
           {:ok, fresh_owner} <- operator(actor, connection, System.system_time(:second)),
           {:ok, decided} <-
             Store.change(
               owner["tenant_id"],
               current["id"],
               &decide_stored(&1, current, fresh_owner, decision, input["decision_id"]),
               now
             ),
           {:ok, _} <- operator(actor, connection, System.system_time(:second)) do
        deliver(actor, connection, decided, mode)
      else
        false -> {:error, :invalid_request}
        error -> error
      end
    end)
  end

  defp decide_stored(stored, current, owner, decision, id) do
    with :ok <- expected(stored, current), :ok <- decision_retry(stored, decision, id) do
      if stored["state"] == "pending",
        do: Review.decide(stored, owner, decision, id, System.system_time(:second)),
        else: {:ok, stored}
    end
  end

  defp edit_current(owner, current, input, now) do
    retry_hash =
      Owner.digest([
        input["digest"],
        input["version"],
        input["proposal"],
        input["idempotency_key"]
      ])

    if current["version"] == input["version"] + 1 and current["request_sha256"] == retry_hash do
      with :ok <- source_active(owner, current), do: {:ok, current}
    else
      with :ok <- expected(current, input),
           :ok <-
             Admission.admit(owner, "replay.prepare", input["idempotency_key"], input, 1, now),
           {:ok, source} <- source(owner, input["proposal"], now) do
        Store.change(
          owner["tenant_id"],
          current["id"],
          &revise_stored(&1, current, owner, source, input, now),
          now
        )
      end
    end
  end

  defp revise_stored(stored, current, owner, source, input, now) do
    with :ok <- expected(stored, current) do
      Review.revise(stored, owner, input["proposal"], source, input["idempotency_key"], now)
    end
  end

  def workspace(actor, connection, now) do
    CustomerReviewDeadline.run(actor, fn ->
      with {:ok, owner} <- Sources.owner(actor, connection, now, "read_review"),
           {:ok, records, _} <- Store.load(owner["tenant_id"]),
           {:ok, _} <-
             Sources.owner(actor, connection, System.system_time(:second), "read_review") do
        summaries =
          records
          |> Map.values()
          |> Enum.filter(&Owner.matches?(&1, owner))
          |> Enum.sort_by(&{&1["updated_at"], &1["id"]}, :desc)
          |> Enum.map(fn record ->
            Map.put(Review.reference(record), "effective_state", effective(record, now))
          end)

        {:ok, %{reviews: summaries}}
      end
    end)
  end

  defp do_prepare(authorize, input, now) do
    with true <-
           keys?(input, ~w(expected_revision idempotency_key proposal)) and
             input["expected_revision"] === 0,
         true <- Operation.valid_at?(input["idempotency_key"], now),
         {:ok, owner} <- authorize.(now),
         {:ok, stored} <- prepare_current(owner, input, now),
         {:ok, _} <- authorize.(System.system_time(:second)),
         :ok <- source_active(owner, stored) do
      if stored["state"] == "pending",
        do: {:ok, display(stored, "pending", nil)},
        else: result(stored)
    else
      false -> {:error, :invalid_request}
      error -> error
    end
  end

  defp prepare_current(owner, input, now) do
    id = Owner.object_id(owner, "replay-review", input["idempotency_key"])

    case Store.fetch(owner["tenant_id"], id) do
      {:ok, stored} ->
        if Owner.matches?(stored, owner) and
             stored["request_sha256"] == Owner.digest(input["proposal"]),
           do: recover_preparation(owner, stored, input, now),
           else: {:error, :review_conflict}

      {:error, :not_found} ->
        with :ok <-
               Admission.admit(owner, "replay.prepare", input["idempotency_key"], input, 1, now),
             {:ok, source} <- source(owner, input["proposal"], now),
             {:ok, candidate} <-
               Review.new(owner, input["proposal"], source, input["idempotency_key"], now) do
          Store.insert(owner["tenant_id"], candidate, now)
        end

      error ->
        error
    end
  end

  defp recover_preparation(owner, %{"state" => "pending"} = stored, input, now) do
    with :ok <- Admission.admit(owner, "replay.prepare", input["idempotency_key"], input, 1, now),
         {:ok, _} <- source(owner, stored["proposal"], now) do
      {:ok, stored}
    end
  end

  defp recover_preparation(_owner, stored, _input, _now), do: {:ok, stored}

  defp do_read(authorize, input, now) do
    with true <-
           keys?(input, ~w(digest id request_id version)) and
             Operation.valid_at?(input["request_id"], now),
         {:ok, owner} <- authorize.(now),
         {:ok, record} <- owned(owner, input),
         {:ok, response} <- current_view(owner, record, input["request_id"], now),
         {:ok, _} <- authorize.(System.system_time(:second)) do
      {:ok, response}
    else
      false -> {:error, :invalid_request}
      error -> error
    end
  end

  defp current_view(owner, record, request, now) do
    if effective(record, now) == "expired" do
      {:ok, Map.put(Review.reference(record), "effective_state", "expired")}
    else
      with :ok <- source_active(owner, record),
           {:ok, current, _} <- Store.load(owner["tenant_id"]),
           true <- current[record["id"]] == record do
        if record["state"] != "pending",
          do: result(record),
          else: pending_view(owner, record, request, now)
      else
        false -> {:error, :review_conflict}
        error -> error
      end
    end
  end

  defp pending_view(owner, record, request, now) do
    with :ok <-
           Admission.admit(
             owner,
             "replay.read",
             request,
             [record["id"], record["digest"]],
             1,
             now
           ),
         {:ok, _} <- source(owner, record["proposal"], now) do
      {:ok, display(record, "pending", nil)}
    else
      {:error, :source_changed} -> {:ok, display(record, "superseded", nil)}
      {:error, :source_expired} -> {:ok, display(record, "expired", nil)}
      error -> error
    end
  end

  defp decision_freshness(_owner, record, _input, "rejected", now),
    do: if(effective(record, now) == "expired", do: {:error, :review_expired}, else: {:ok, :none})

  defp decision_freshness(owner, %{"state" => "approved"} = record, input, "approved", now) do
    with :ok <- source_active(owner, record) do
      case TrackedReplays.get(record["tenant_id"], record["replay_operation_id"]) do
        {:ok, _} -> {:ok, :recover}
        {:error, :not_found} -> fresh_for_dispatch(owner, record, input, now)
        error -> error
      end
    end
  end

  defp decision_freshness(owner, record, input, "approved", now) do
    fresh_for_dispatch(owner, record, input, now)
  end

  defp fresh_for_dispatch(owner, record, input, now) do
    with true <- now < record["expires_at"],
         :ok <-
           Admission.admit(
             owner,
             "replay.approve",
             input["request_id"],
             [record["id"], record["digest"]],
             1,
             now
           ),
         {:ok, _} <- source(owner, record["proposal"], now) do
      {:ok, :dispatch}
    else
      false -> {:error, :review_expired}
      error -> error
    end
  end

  defp deliver(_actor, _connection, record, :none),
    do:
      {:ok,
       Review.reference(record)
       |> Map.merge(%{
         "effective_state" => "rejected",
         "decision" => record["decision"],
         "replay" => nil
       })}

  defp deliver(_actor, _connection, record, :recover),
    do: result(record)

  defp deliver(actor, connection, record, :dispatch) do
    with true <- System.system_time(:second) < record["expires_at"],
         {:ok, owner} <- operator(actor, connection, System.system_time(:second)),
         :ok <- Sources.enabled(owner, record["action"]["projection_name"]),
         :ok <- source_active(owner, record),
         {:ok, replay} <-
           TrackedReplays.start(
             record["tenant_id"],
             record["action"]["projection_name"],
             record["replay_operation_id"]
           ) do
      {:ok, display(record, "approved", replay)}
    else
      false -> {:error, :review_expired}
      error -> error
    end
  end

  defp result(%{"state" => "rejected"} = record), do: {:ok, display(record, "rejected", nil)}

  defp result(record) do
    case TrackedReplays.get(record["tenant_id"], record["replay_operation_id"]) do
      {:ok, replay} ->
        {:ok, display(record, record["state"], replay)}

      {:error, :not_found} ->
        {:ok, display(record, record["state"], %{"status" => "not_started"})}

      error ->
        error
    end
  end

  defp source(owner, proposal, now) do
    with {:ok, %{kind: "replay_plan", sources: [reference], projection_name: target}} <-
           Proposal.validate(proposal),
         {:ok, source} <- Sources.resolve(owner, reference, now),
         true <- source["locator"] == target do
      {:ok, source}
    else
      {:error, _} = error -> error
      _ -> {:error, :invalid_review}
    end
  end

  defp source_active(owner, record) do
    [reference] = record["proposal"]["sources"]

    with :ok <- Records.active?(owner["tenant_id"], "sources", reference["ref"]),
         {:ok, source} <- Records.fetch(owner["tenant_id"], "sources", reference["ref"]) do
      EvidenceSource.authorize(source, owner, reference, System.system_time(:second))
    end
  end

  defp operator(actor, connection, now) do
    with {:ok, owner} <- Sources.owner(actor, connection, now),
         true <- owner["membership_role"] == "admin" do
      {:ok, owner}
    else
      false -> {:error, :access_denied}
      error -> error
    end
  end

  defp agent_owner(token, binding, operation, now) do
    with true <- operation in ~w(prepare_proposal read_review read_result),
         {:ok, owner} <- CustomerAgentAccess.verify_metered(token, binding, operation, now),
         true <- owner["consent_version"] == ConnectionConsent.replay_version() do
      {:ok, owner}
    else
      false -> {:error, :access_denied}
      error -> error
    end
  end

  defp owned(owner, input) do
    with {:ok, record} <- Store.fetch(owner["tenant_id"], input["id"]),
         true <- Owner.matches?(record, owner),
         true <- record["version"] === input["version"] and record["digest"] == input["digest"] do
      {:ok, record}
    else
      false -> {:error, :review_conflict}
      error -> error
    end
  end

  defp expected(current, expected) when is_map(current),
    do:
      if(current["digest"] == expected["digest"] and current["version"] == expected["version"],
        do: :ok,
        else: {:error, :review_conflict}
      )

  defp expected(_, _), do: {:error, :review_conflict}
  defp decision_retry(%{"state" => "pending"}, _, _), do: :ok
  defp decision_retry(%{"state" => state, "decision" => %{"id" => id}}, state, id), do: :ok
  defp decision_retry(_, _, _), do: {:error, :review_conflict}

  defp effective(record, now),
    do: if(now >= record["expires_at"], do: "expired", else: record["state"])

  defp keys?(value, keys), do: is_map(value) and Enum.sort(Map.keys(value)) == keys

  defp display(record, state, replay),
    do:
      Review.reference(record)
      |> Map.merge(%{
        "effective_state" => state,
        "proposal" => record["proposal"],
        "evidence" => record["snapshot"],
        "action" => record["action"],
        "decision" => record["decision"],
        "replay" => replay,
        "approved_version" => if(record["state"] == "approved", do: record["version"], else: nil),
        "approved_digest" => if(record["state"] == "approved", do: record["digest"], else: nil)
      })
end
