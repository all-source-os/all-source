defmodule QueryServiceEx.Integration.CustomerEvidenceReviewTest do
  use ExUnit.Case, async: false
  alias QueryServiceEx.Application.Services.AgentRunEvidence
  alias QueryServiceEx.Application.Services.AgentRunRecorder
  alias QueryServiceEx.Application.Services.CustomerEvidenceReview, as: Review
  alias QueryServiceEx.Application.Services.CustomerEvidenceSources, as: Sources
  alias QueryServiceEx.Application.Services.CustomerReviewRecords, as: Records
  alias QueryServiceEx.Domain.CustomerAgent.ConnectionConsent
  alias QueryServiceEx.Infrastructure.Adapters.CustomerAgentGrantStore, as: Grants
  alias QueryServiceEx.Infrastructure.Adapters.CustomerQueryUsageStore, as: Usage
  alias QueryServiceEx.Infrastructure.Adapters.CustomerReviewStore, as: Store
  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient
  alias QueryServiceEx.TestSupport.AgentRunFixture, as: F
  alias QueryServiceEx.TestSupport.CustomerAgentCore
  import QueryServiceEx.TestSupport.CustomerAgentCore, only: [with_core: 2]
  @moduletag :integration
  @moduletag timeout: 60_000
  @moduletag skip: is_nil(System.get_env("ALLSOURCE_CORE_BINARY"))
  @tenant "synthetic-evidence-review"
  @subject "oauth:google:synthetic-owner"
  @actor %{"tenant_id" => @tenant, "subject_id" => @subject}
  @binding Map.merge(@actor, %{
             "client_id" => "claude-code",
             "resource" => "https://example.test/customer-review"
           })

  defmodule LostWriteReply do
    defdelegate load(tenant), to: Store
    defdelegate active?(tenant, kind, id), to: Store
    defdelegate revoke(tenant, kind, id), to: Store

    def replace(tenant, value, revision) do
      send(Application.fetch_env!(:query_service_ex, :review_test_owner), :review_write_attempted)
      :ok = Store.replace(tenant, value, revision)
      {:error, :storage_unavailable}
    end
  end

  setup do
    context = CustomerAgentCore.setup_context()
    previous = Application.get_env(:query_service_ex, :customer_review_resource)
    previous_store = Application.fetch_env!(:query_service_ex, :customer_review_store)
    previous_owner = Application.get_env(:query_service_ex, :review_test_owner)
    Application.put_env(:query_service_ex, :customer_review_resource, @binding["resource"])

    on_exit(fn ->
      Application.put_env(:query_service_ex, :customer_review_store, previous_store)

      if previous_owner,
        do: Application.put_env(:query_service_ex, :review_test_owner, previous_owner),
        else: Application.delete_env(:query_service_ex, :review_test_owner)

      if previous,
        do: Application.put_env(:query_service_ex, :customer_review_resource, previous),
        else: Application.delete_env(:query_service_ex, :customer_review_resource)
    end)

    context
  end

  test "lost draft write response recovers without another write", context do
    with_core(context, fn ->
      provision()
      now = System.system_time(:second)
      grant = grant(now)
      input = prepare_input(grant, now)
      Application.put_env(:query_service_ex, :review_test_owner, self())
      Application.put_env(:query_service_ex, :customer_review_store, LostWriteReply)
      assert {:error, :storage_unavailable} = Review.prepare(grant.token, @binding, input, now)
      assert_receive :review_write_attempted

      assert {:ok, %{state: "pending", approved: false}} =
               Review.prepare(grant.token, @binding, input, now)

      refute_receive :review_write_attempted
      assert {:ok, workspace, _} = Store.load(@tenant)
      assert map_size(workspace["reviews"]) == 1
    end)
  end

  test "selected runs prepare one owner-scoped pending review and recover after hard restart",
       context do
    {grant, input, receipt, view} =
      with_core(context, fn ->
        provision()
        now = System.system_time(:second)
        grant = grant(now)
        input = prepare_input(grant, now)
        assert {:ok, receipt} = Review.prepare(grant.token, @binding, input, now)
        assert receipt.state == "pending"
        assert receipt.persisted and not receipt.approved
        assert receipt.execution == "none"
        assert {:ok, ^receipt} = Review.prepare(grant.token, @binding, input, now + 1)
        assert {:ok, view} = read_review(grant.token, @binding, receipt.id, 1, now)
        assert view.evidence.state == "divergent"
        assert view.evidence.first_divergence.change_number == 1

        assert {:ok, %{result_available: false, approved: false}} =
                 read_review(grant.token, @binding, receipt.id, 1, now, "read_result")

        assert {:ok, workspace, _} = Store.load(@tenant)
        assert map_size(workspace["reviews"]) == 1
        refute Jason.encode!(workspace["reviews"]) =~ F.uuid(1)
        refute Jason.encode!(workspace) =~ grant.token
        {grant, input, receipt, view}
      end)

    with_core(context, fn ->
      now = System.system_time(:second)
      assert {:ok, ^receipt} = Review.prepare(grant.token, @binding, input, now)
      assert {:ok, ^view} = read_review(grant.token, @binding, receipt.id, 1, now)
      assert {:ok, workspace, _} = Store.load(@tenant)
      assert map_size(workspace["reviews"]) == 1
    end)
  end

  test "source deletion remains denied after stale workspace restore and hard restart", context do
    {grant, receipt, reference} =
      with_core(context, fn ->
        provision()
        now = System.system_time(:second)
        grant = grant(now)
        input = prepare_input(grant, now)
        assert {:ok, receipt} = Review.prepare(grant.token, @binding, input, now)
        reference = hd(input["proposal"]["sources"])
        assert {:ok, old, revision} = Store.load(@tenant)
        assert :ok = Records.revoke(@tenant, "sources", reference["ref"])
        assert :ok = Records.revoke(@tenant, "sources", reference["ref"])
        assert :ok = Store.replace(@tenant, old, revision)
        assert {:error, :source_denied} = Review.prepare(grant.token, @binding, input, now)

        assert {:ok, %{state: "unavailable"} = unavailable} =
                 read_review(grant.token, @binding, receipt.id, 1, now)

        refute Map.has_key?(unavailable, :evidence)
        {grant, receipt, reference}
      end)

    with_core(context, fn ->
      assert {:error, :revoked} = Records.fetch(@tenant, "sources", reference["ref"])

      assert {:ok, %{state: "unavailable"}} =
               read_review(grant.token, @binding, receipt.id, 1, System.system_time(:second))

      assert :ok = Records.revoke(@tenant, "reviews", receipt.id)

      assert {:error, :access_denied} =
               read_review(grant.token, @binding, receipt.id, 1, System.system_time(:second))
    end)
  end

  test "different connection or owner, old consent and revoked membership cannot disclose reviews",
       context do
    with_core(context, fn ->
      provision()
      now = System.system_time(:second)
      grant = grant(now)
      input = prepare_input(grant, now)
      assert {:ok, receipt} = Review.prepare(grant.token, @binding, input, now)
      other = grant(now)
      assert {:error, :source_denied} = Review.prepare(other.token, @binding, input, now)
      assert {:error, :access_denied} = read_review(other.token, @binding, receipt.id, 1, now)

      assert {:error, :access_denied} =
               read_review(
                 grant.token,
                 Map.put(@binding, "subject_id", "outsider"),
                 receipt.id,
                 1,
                 now
               )

      assert {:error, :access_denied} =
               read_review(
                 grant.token,
                 Map.put(@binding, "tenant_id", "other-tenant"),
                 receipt.id,
                 1,
                 now
               )

      assert {:ok, old} =
               Grants.issue(
                 @binding,
                 ConnectionConsent.operations(),
                 %{"accepted" => true, "version" => ConnectionConsent.version()},
                 now,
                 600
               )

      assert {:error, :access_denied} = Review.prepare(old.token, @binding, input, now)

      assert {:error, :access_denied} =
               Sources.share(
                 @actor,
                 old.id,
                 %{
                   "consent" => %{"accepted" => true, "version" => "selected-run-evidence-v1"},
                   "operation_id" => "#{now}:#{F.uuid(99)}",
                   "run_id" => F.uuid(1),
                   "revision" => 7,
                   "sha256" => F.hash(1),
                   "ttl" => 60
                 },
                 now
               )

      set_members([])
      assert {:error, :access_denied} = read_review(grant.token, @binding, receipt.id, 1, now)
    end)
  end

  test "changed evidence supersedes the pinned view and expiry hides its contents", context do
    with_core(context, fn ->
      provision()
      now = System.system_time(:second)
      grant = grant(now)
      input = prepare_input(grant, now, 3)
      assert {:ok, receipt} = Review.prepare(grant.token, @binding, input, now)
      assert {:ok, baseline} = AgentRunEvidence.read(@tenant, F.uuid(1))

      assert {:ok, _} =
               AgentRunRecorder.record(@tenant, F.uuid(1), %{
                 "operation_id" => F.uuid(190),
                 "expected_version" => 3,
                 "event" => F.payload("capture_gap", F.uuid(1), List.last(baseline.events)["id"])
               })

      assert {:ok, %{state: "superseded"} = stale} =
               read_review(grant.token, @binding, receipt.id, 1, now)

      refute Map.has_key?(stale, :evidence)
      assert {:error, :source_changed} = Review.prepare(grant.token, @binding, input, now)

      assert {:ok, %{state: "expired"} = expired} =
               read_review(grant.token, @binding, receipt.id, 1, now + 61)

      refute Map.has_key?(expired, :proposal)
    end)
  end

  test "concurrent preparation retries preserve one exact pending record", context do
    with_core(context, fn ->
      provision()
      now = System.system_time(:second)
      grant = grant(now)
      input = prepare_input(grant, now)

      tasks =
        for _ <- 1..4, do: Task.async(fn -> Review.prepare(grant.token, @binding, input, now) end)

      results = Task.await_many(tasks, 30_000)
      assert Enum.all?(results, &match?({:ok, %{state: "pending", approved: false}}, &1))
      assert results |> Enum.uniq() |> length() == 1
      assert {:ok, %{"used" => 4}} = Usage.snapshot(@tenant)
      assert {:ok, workspace, _} = Store.load(@tenant)
      assert map_size(workspace["reviews"]) == 1
      reversed = update_in(input["proposal"]["sources"], &Enum.reverse/1)

      assert {:error, :idempotency_conflict} =
               Review.prepare(grant.token, @binding, reversed, now)

      assert {:ok, %{"used" => 4}} = Usage.snapshot(@tenant)

      assert {:error, :invalid_preparation} =
               Review.prepare(grant.token, @binding, Map.put(input, "approved", true), now)
    end)
  end

  defp prepare_input(grant, now, count \\ 7) do
    refs =
      for {number, outcome} <- [{1, "pass"}, {2, "fail"}] do
        run_id = F.uuid(number)

        last_id =
          F.history(run_id, outcome)
          |> Enum.take(count)
          |> Enum.reduce(nil, fn event, previous ->
            input = %{
              "operation_id" => F.uuid(800 + event["version"]),
              "expected_version" => event["version"] - 1,
              "event" => Map.put(event["payload"], "causation_id", previous)
            }

            assert {:ok, receipt} = AgentRunRecorder.record(@tenant, run_id, input)
            receipt.event_id
          end)

        assert {:ok, run} = AgentRunEvidence.read(@tenant, run_id)
        assert List.last(run.events)["id"] == last_id

        assert {:ok, shared} =
                 Sources.share(
                   @actor,
                   grant.id,
                   %{
                     "consent" => %{"accepted" => true, "version" => "selected-run-evidence-v1"},
                     "operation_id" => "#{now}:#{F.uuid(900 + number)}",
                     "run_id" => run_id,
                     "revision" => run.revision,
                     "sha256" => run.digest,
                     "ttl" => 60
                   },
                   now
                 )

        shared.source
      end

    %{
      "expected_revision" => 0,
      "idempotency_key" => "#{now}:#{F.uuid(999)}",
      "proposal" => %{
        "schema_version" => 1,
        "kind" => "run_comparison",
        "projection_name" => nil,
        "sources" => refs
      }
    }
  end

  defp read_review(token, binding, id, version, now, operation \\ "read_review") do
    request_id = "#{now}:#{F.uuid(3_000)}"
    Review.read(token, binding, id, version, request_id, now, operation)
  end

  defp grant(now) do
    assert {:ok, grant} =
             Grants.issue(
               @binding,
               ConnectionConsent.evidence_operations(),
               %{"accepted" => true, "version" => ConnectionConsent.evidence_version()},
               now,
               600
             )

    grant
  end

  defp provision do
    assert {:ok, %{status: 201}} =
             Tesla.post(RustCoreClient.write_client(), "/api/v1/tenants", %{
               id: @tenant,
               name: "Synthetic evidence review"
             })

    set_members([%{"user_id" => @subject, "role" => "member"}])

    assert {:ok, %{status: 200}} =
             Tesla.put(RustCoreClient.write_client(), "/api/v1/tenants/#{@tenant}", %{
               metadata: %{
                 "subscription" => %{"tier" => "indie", "status" => "active"},
                 "quotas" => %{"mcp_scope" => "read", "queries_quota" => 100, "queries_used" => 0}
               }
             })
  end

  defp set_members(members),
    do:
      assert(
        {:ok, %{status: 200}} =
          Tesla.post(RustCoreClient.write_client(), "/api/v1/config", %{
            key: "team:#{@tenant}:members",
            value: %{"schema_version" => 2, "members" => members, "invitations" => %{}},
            changed_by: "synthetic-test"
          })
      )
end
