defmodule QueryServiceEx.Integration.CustomerReplayReviewTest do
  use ExUnit.Case, async: false
  alias QueryServiceEx.Application.Services.CustomerReplayReview, as: Review
  alias QueryServiceEx.Application.Services.CustomerReviewRecords, as: Records
  alias QueryServiceEx.Infrastructure.Adapters.CustomerActionStore, as: Store
  alias QueryServiceEx.Infrastructure.Adapters.CustomerAgentGrantStore, as: Grants
  alias QueryServiceEx.Projections.Catalog
  alias QueryServiceEx.Projections.Enablement
  alias QueryServiceEx.Projections.TenantProjections, as: Engine
  alias QueryServiceEx.Projections.TrackedReplays
  alias QueryServiceEx.TestSupport.AgentRunFixture, as: Run
  alias QueryServiceEx.TestSupport.CustomerAgentCore, as: Core
  alias QueryServiceEx.TestSupport.CustomerRemoteHTTP, as: HTTP
  alias QueryServiceEx.TestSupport.CustomerReplayFixture, as: R
  alias QueryServiceEx.TestSupport.MeteredEvidenceFixture, as: F
  @moduletag :integration
  @moduletag timeout: 120_000
  @moduletag skip: is_nil(System.get_env("ALLSOURCE_CORE_BINARY"))
  setup do: R.setup_context()

  test "human approval dispatches one real replay and survives duplicate calls and Core restart",
       context do
    {grant, read, result} =
      Core.with_core(context, fn ->
        now = System.system_time(:second)
        {grant, _} = R.provision(now)
        input = R.prepare_input(grant, now)
        assert {:ok, pending} = Review.prepare(grant.token, F.binding(), input, now)
        assert pending["state"] == "pending"
        assert pending["decision"] == nil
        assert Engine.list_replays(F.tenant()) == []
        assert pending["action"]["history"] == "retained_at_dispatch"
        assert "archive_completeness" in pending["evidence"]["analysis"]["unknowns"]
        refute inspect(pending) =~ "untrusted:"
        decision = decision(pending, now)
        assert {200, %{"data" => approved}, _} = R.post(context, "approve", grant, decision)
        assert approved["approved_digest"] == pending["digest"]
        assert approved["decision"]["actor"] == F.actor()["subject_id"]

        R.wait_until(fn ->
          match?(
            {:ok, %{"status" => "completed"}},
            TrackedReplays.get(F.tenant(), pending["replay_operation_id"])
          )
        end)

        assert {:ok, %{"total" => 1}} =
                 Engine.get_state(F.tenant(), "event-count", Catalog.tenant_key())

        assert {200, %{"data" => completed}, _} = R.post(context, "approve", grant, decision)
        assert length(Engine.list_replays(F.tenant())) == 1
        assert completed["replay"]["status"] == "completed"

        assert {409, _, _} =
                 R.post(context, "approve", grant, %{decision | "decision_id" => Run.uuid(800)})

        assert {:ok, ^completed} = Review.prepare(grant.token, F.binding(), input, now)
        read = read(pending, now)
        assert {:ok, ^completed} = Review.read(grant.token, F.binding(), read, now, "read_result")
        {grant, read, completed}
      end)

    Core.with_core(context, fn ->
      assert {:ok, ^result} =
               Review.read(
                 grant.token,
                 F.binding(),
                 read,
                 System.system_time(:second),
                 "read_result"
               )

      assert {200, %{"data" => ^result}, _} = R.post(context, "read", grant, read)
    end)
  end

  test "session alone, agent token and proofs for other bodies or actions cannot decide",
       context do
    Core.with_core(context, fn ->
      now = System.system_time(:second)
      {grant, _} = R.provision(now)

      assert {:ok, pending} =
               Review.prepare(grant.token, F.binding(), R.prepare_input(grant, now), now)

      body = %{"connection_id" => grant.id, "input" => decision(pending, now)}
      token = R.session()

      for headers <- [
            HTTP.bearer(token),
            HTTP.bearer(grant.token),
            HTTP.bearer(token) ++ [{"x-allsource-product-action", R.proof(token, "reject", body)}],
            HTTP.bearer(token) ++ [{"x-allsource-product-action", R.proof(token, "approve", %{})}],
            HTTP.bearer(token) ++
              [{"x-allsource-product-action", R.proof("other-session", "approve", body)}],
            HTTP.bearer(token) ++
              [{"x-allsource-product-action", R.proof(token, "approve", body, %{"exp" => now})}]
          ] do
        assert {403, _, _} = HTTP.http(context, :post, "replay/approve", body, headers)
      end

      assert {403, _, _} = R.post(context, "approve", grant, body["input"], grant.token)
      assert {:ok, %{"state" => "pending"}} = Store.fetch(F.tenant(), pending["id"])
      assert Engine.list_replays(F.tenant()) == []
    end)
  end

  test "new source facts supersede approval; rejection preserves accepted state", context do
    Core.with_core(context, fn ->
      now = System.system_time(:second)
      {grant, _} = R.provision(now)

      assert {:ok, pending} =
               Review.prepare(grant.token, F.binding(), R.prepare_input(grant, now), now)

      R.ingest("second")

      assert {:ok, %{"effective_state" => "superseded"}} =
               Review.read(grant.token, F.binding(), read(pending, now), now)

      assert {409, _, _} = R.post(context, "approve", grant, decision(pending, now))
      assert {:ok, %{"state" => "pending"}} = Store.fetch(F.tenant(), pending["id"])

      assert {200, %{"data" => rejected}, _} =
               R.post(context, "reject", grant, decision(pending, now))

      assert rejected["effective_state"] == "rejected"
      refute Map.has_key?(rejected, "evidence")
      assert Engine.list_replays(F.tenant()) == []

      assert {:ok, %{"total" => 1}} =
               Engine.get_state(F.tenant(), "event-count", Catalog.tenant_key())
    end)
  end

  test "live role, projection, source and connection consent are enforced", context do
    Core.with_core(context, fn ->
      now = System.system_time(:second)
      {grant, old} = R.provision(now)
      input = R.prepare_input(grant, now)
      assert {:error, :access_denied} = Review.prepare(old.token, F.binding(), input, now)
      assert {:ok, pending} = Review.prepare(grant.token, F.binding(), input, now)
      R.role("member")
      assert {403, _, _} = R.post(context, "approve", grant, decision(pending, now))
      R.role("admin")
      assert {:ok, _} = Enablement.disable(F.tenant(), "event-count")
      assert {409, _, _} = R.post(context, "approve", grant, decision(pending, now))
      R.enable("event-count")
      [source] = input["proposal"]["sources"]
      assert :ok = Records.revoke(F.tenant(), "sources", source["ref"])
      assert {403, _, _} = R.post(context, "approve", grant, decision(pending, now))
      assert {403, _, _} = R.post(context, "read", grant, read(pending, now))

      assert {200, %{"data" => rejected}, _} =
               R.post(context, "reject", grant, decision(pending, now))

      refute Map.has_key?(rejected, "evidence")
      assert Engine.list_replays(F.tenant()) == []
    end)
  end

  test "meaningful edits increment version, recover identical retry and deny old approval",
       context do
    Core.with_core(context, fn ->
      now = System.system_time(:second)
      {grant, _} = R.provision(now)
      R.enable("entity-activity")

      assert {:ok, pending} =
               Review.prepare(grant.token, F.binding(), R.prepare_input(grant, now), now)

      changed = R.prepare_input(grant, now, 2, "entity-activity")

      edit =
        Map.take(pending, ~w(id digest version))
        |> Map.merge(%{
          "proposal" => changed["proposal"],
          "idempotency_key" => F.operation(333, now)
        })

      assert {200, %{"data" => revised}, _} = R.post(context, "edit", grant, edit)
      assert revised["version"] == 2
      refute revised["digest"] == pending["digest"]
      assert revised["replay_operation_id"] == pending["replay_operation_id"]
      assert {200, %{"data" => ^revised}, _} = R.post(context, "edit", grant, edit)
      assert {422, _, _} = R.post(context, "edit", grant, %{edit | "version" => "1"})
      assert {409, _, _} = R.post(context, "approve", grant, decision(pending, now))

      assert {200, %{"data" => approved}, _} =
               R.post(context, "approve", grant, decision(revised, now))

      assert approved["approved_version"] == 2
      assert approved["action"]["projection_name"] == "entity-activity"
    end)
  end

  test "lost approval acknowledgement recovers committed receipt and dispatches once", context do
    {grant, input, pending} =
      Core.with_core(context, fn ->
        now = System.system_time(:second)
        {grant, _} = R.provision(now)

        assert {:ok, pending} =
                 Review.prepare(grant.token, F.binding(), R.prepare_input(grant, now), now)

        input = decision(pending, now)
        R.lose_approval_ack(context)
        assert {503, _, _} = R.post(context, "approve", grant, input)
        assert {:ok, %{"state" => "approved"}} = Store.fetch(F.tenant(), pending["id"])
        assert Engine.list_replays(F.tenant()) == []
        {grant, input, pending}
      end)

    Application.put_env(:query_service_ex, :core_write_url, context.url)

    Core.with_core(context, fn ->
      assert {200, %{"data" => result}, _} = R.post(context, "approve", grant, input)
      assert result["approved_digest"] == pending["digest"]

      R.wait_until(fn ->
        match?(
          {:ok, %{"status" => "completed"}},
          TrackedReplays.get(F.tenant(), pending["replay_operation_id"])
        )
      end)

      assert {200, _, _} = R.post(context, "approve", grant, input)
      assert length(Engine.list_replays(F.tenant())) == 1
    end)
  end

  test "competing decisions cannot fork replay identity", context do
    Core.with_core(context, fn ->
      now = System.system_time(:second)
      {grant, _} = R.provision(now)

      assert {:ok, pending} =
               Review.prepare(grant.token, F.binding(), R.prepare_input(grant, now), now)

      replies =
        1..6
        |> Task.async_stream(
          fn n ->
            input = %{
              decision(pending, now)
              | "decision_id" => Run.uuid(710 + n),
                "request_id" => F.operation(710 + n, now)
            }

            Review.decide(F.actor(), grant.id, input, "approved", now)
          end,
          max_concurrency: 6,
          timeout: 20_000
        )
        |> Enum.map(fn {:ok, result} -> result end)

      assert Enum.count(replies, &match?({:ok, _}, &1)) == 1
      assert Enum.all?(replies, &(match?({:ok, _}, &1) or match?({:error, _}, &1)))
      assert {:ok, stored} = Store.fetch(F.tenant(), pending["id"])
      assert stored["state"] == "approved"
      assert stored["digest"] == pending["digest"]
      assert length(Engine.list_replays(F.tenant())) == 1
    end)
  end

  test "concurrent edit and approval cannot apply different content under one receipt", context do
    Core.with_core(context, fn ->
      now = System.system_time(:second)
      {grant, _} = R.provision(now)
      R.enable("entity-activity")

      assert {:ok, pending} =
               Review.prepare(grant.token, F.binding(), R.prepare_input(grant, now), now)

      changed = R.prepare_input(grant, now, 2, "entity-activity")

      edit =
        Map.take(pending, ~w(id version digest))
        |> Map.merge(%{
          "proposal" => changed["proposal"],
          "idempotency_key" => F.operation(820, now)
        })

      results =
        [
          fn -> Review.edit(F.actor(), grant.id, edit, now) end,
          fn -> Review.decide(F.actor(), grant.id, decision(pending, now), "approved", now) end
        ]
        |> Task.async_stream(& &1.(), max_concurrency: 2, timeout: 20_000)
        |> Enum.map(fn {:ok, result} -> result end)

      assert Enum.count(results, &match?({:ok, _}, &1)) == 1
      assert {:ok, stored} = Store.fetch(F.tenant(), pending["id"])

      case stored["state"] do
        "approved" ->
          assert stored["version"] == 1
          assert stored["decision"]["digest"] == pending["digest"]
          assert stored["action"]["projection_name"] == "event-count"
          assert length(Engine.list_replays(F.tenant())) == 1

        "pending" ->
          assert stored["version"] == 2
          assert stored["decision"] == nil
          assert stored["action"]["projection_name"] == "entity-activity"
          assert Engine.list_replays(F.tenant()) == []
      end
    end)
  end

  test "foreign sessions, expired reviews and revoked grants cannot expose or dispatch",
       context do
    Core.with_core(context, fn ->
      now = System.system_time(:second)
      {grant, _} = R.provision(now)

      assert {:ok, pending} =
               Review.prepare(grant.token, F.binding(), R.prepare_input(grant, now), now)

      for extra <- [%{"sub" => "oauth:google:someone-else"}, %{"tenant_id" => "different-tenant"}] do
        token =
          HTTP.session(
            Map.merge(%{"sub" => F.actor()["subject_id"], "tenant_id" => F.tenant()}, extra)
          )

        assert {403, _, _} = R.post(context, "read", grant, read(pending, now), token)
        assert {403, _, _} = R.post(context, "approve", grant, decision(pending, now), token)
      end

      expired = pending["expires_at"]

      assert {:error, :review_expired} =
               Review.decide(F.actor(), grant.id, decision(pending, expired), "approved", expired)

      assert {:ok, hidden} =
               Review.read(grant.token, F.binding(), read(pending, expired), expired)

      assert hidden["effective_state"] == "expired"
      refute Map.has_key?(hidden, "evidence")
      assert :ok = Grants.revoke(F.binding(), grant.id, now)
      assert {403, _, _} = R.post(context, "read", grant, read(pending, now))
      assert Engine.list_replays(F.tenant()) == []
    end)
  end

  defp decision(review, now),
    do:
      Map.take(review, ~w(id digest version))
      |> Map.merge(%{
        "decision_id" => Run.uuid(700),
        "request_id" => F.operation(701, now)
      })

  defp read(review, now),
    do: Map.take(review, ~w(id digest version)) |> Map.put("request_id", F.operation(702, now))
end
