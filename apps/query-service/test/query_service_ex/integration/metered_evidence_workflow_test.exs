defmodule QueryServiceEx.Integration.MeteredEvidenceWorkflowTest do
  use ExUnit.Case, async: false
  alias QueryServiceEx.Application.Services.CustomerEvidenceReview, as: Review
  alias QueryServiceEx.Application.Services.CustomerEvidenceSources, as: Sources
  alias QueryServiceEx.Application.Services.CustomerReviewRecords, as: Records
  alias QueryServiceEx.Infrastructure.Adapters.CustomerQueryOperationStore, as: Journal
  alias QueryServiceEx.Infrastructure.Adapters.CustomerQueryUsageStore, as: Usage
  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient
  alias QueryServiceEx.TestSupport.MeteredEvidenceFixture, as: F
  import QueryServiceEx.TestSupport.CustomerAgentCore, only: [with_core: 2]
  @moduletag :integration
  @moduletag timeout: 60_000
  @moduletag skip: is_nil(System.get_env("ALLSOURCE_CORE_BINARY"))

  defmodule LostJournalReply do
    defdelegate load(tenant), to: Journal

    def replace(tenant, value, revision) do
      send(Application.fetch_env!(:query_service_ex, :meter_test_owner), :journal_write)
      :ok = Journal.replace(tenant, value, revision)
      {:error, :query_usage_unavailable}
    end
  end

  defmodule LostAdmissionReply do
    defdelegate snapshot(tenant), to: Usage

    def admit(tenant, request) do
      {:ok, _} = Usage.admit(tenant, request)
      {:error, :query_usage_unavailable}
    end
  end

  defmodule RevokingSource do
    def events(tenant, run) do
      result = F.ObservedSource.events(tenant, run)
      F.set_members([])
      result
    end
  end

  defmodule UnavailableJournal do
    def load(_tenant), do: {:error, :query_usage_unavailable}
  end

  setup do
    F.setup_context()
  end

  test "source sharing consumes the final unit once and quota refusal performs no source read",
       context do
    with_core(context, fn ->
      now = System.system_time(:second)
      grant = F.provision(1, now)
      input = F.source_input(1, now)
      F.observe_reads()
      assert {:ok, shared} = Sources.share(F.actor(), grant.id, input, now)
      assert_receive {:evidence_read, _}
      assert used() == 1
      assert {:ok, ^shared} = Sources.share(F.actor(), grant.id, input, now)
      assert_receive {:evidence_read, _}
      assert used() == 1
      second = Map.put(input, "operation_id", F.operation(902, now))
      assert {:error, :query_quota_exceeded} = Sources.share(F.actor(), grant.id, second, now)
      refute_receive {:evidence_read, _}
      assert used() == 1
    end)
  end

  test "comparison and read finish on the final unit and replay after hard restart", context do
    {grant, input, receipt, read_id, view, now} =
      with_core(context, fn ->
        now = System.system_time(:second)
        grant = F.provision(4, now)
        input = F.prepare_input(grant, now)
        assert used() == 2
        F.observe_reads()
        assert {:ok, receipt} = Review.prepare(grant.token, F.binding(), input, now)
        assert_receive {:evidence_read, _}
        assert_receive {:evidence_read, _}
        assert used() == 4
        assert {:ok, ^receipt} = Review.prepare(grant.token, F.binding(), input, now)
        assert_receive {:evidence_read, _}
        assert_receive {:evidence_read, _}
        assert used() == 4
        read_id = F.operation(1_000, now)

        assert {:error, :query_quota_exceeded} =
                 Review.read(grant.token, F.binding(), receipt.id, 1, read_id, now)

        refute_receive {:evidence_read, _}
        F.set_quota(6)
        assert {:ok, view} = Review.read(grant.token, F.binding(), receipt.id, 1, read_id, now)
        assert view.evidence.state == "divergent"
        assert used() == 6
        {grant, input, receipt, read_id, view, now}
      end)

    with_core(context, fn ->
      assert {:ok, ^receipt} = Review.prepare(grant.token, F.binding(), input, now)
      assert {:ok, ^view} = Review.read(grant.token, F.binding(), receipt.id, 1, read_id, now)
      assert used() == 6
      assert {:ok, journal, _} = Journal.load(F.tenant())
      assert map_size(journal["operations"]) == 4
      refute Jason.encode!(journal) =~ grant.token
      refute Jason.encode!(journal) =~ "proposal"
    end)
  end

  test "lost journal reply cannot charge or read; retry uses the committed original request",
       context do
    with_core(context, fn ->
      now = System.system_time(:second)
      grant = F.provision(1, now)
      input = F.source_input(1, now)
      F.observe_reads()
      Application.put_env(:query_service_ex, :customer_query_operation_store, LostJournalReply)

      assert {:error, :query_usage_unavailable} = Sources.share(F.actor(), grant.id, input, now)
      assert_receive :journal_write
      refute_receive {:evidence_read, _}
      assert used() == 0
      assert {:ok, _} = Sources.share(F.actor(), grant.id, input, now)
      refute_receive :journal_write
      assert_receive {:evidence_read, _}
      assert used() == 1
    end)
  end

  test "lost admission reply recovers after restart without charging twice", context do
    {grant, input, now} =
      with_core(context, fn ->
        now = System.system_time(:second)
        grant = F.provision(1, now)
        input = F.source_input(1, now)
        F.observe_reads()
        Application.put_env(:query_service_ex, :customer_query_usage_store, LostAdmissionReply)
        assert {:error, :query_usage_unavailable} = Sources.share(F.actor(), grant.id, input, now)
        refute_receive {:evidence_read, _}
        assert used() == 1
        {grant, input, now}
      end)

    Application.put_env(:query_service_ex, :customer_query_usage_store, Usage)

    with_core(context, fn ->
      assert {:ok, _} = Sources.share(F.actor(), grant.id, input, now)
      assert_receive {:evidence_read, _}
      assert used() == 1
    end)
  end

  test "reset cannot rebind a retry to the new generation", context do
    with_core(context, fn ->
      now = System.system_time(:second)
      grant = F.provision(1, now)
      input = F.source_input(1, now)
      F.observe_reads()
      assert {:ok, _} = Sources.share(F.actor(), grant.id, input, now)
      assert_receive {:evidence_read, _}

      assert {:ok, %{status: 200}} =
               Tesla.post(
                 RustCoreClient.write_client(),
                 "/api/v1/tenants/#{F.tenant()}/usage/queries/reset",
                 %{expected_period: 0}
               )

      assert {:error, :query_period_changed} = Sources.share(F.actor(), grant.id, input, now)
      refute_receive {:evidence_read, _}
      assert used() == 0
      next = Map.put(input, "operation_id", F.operation(902, now))
      assert {:ok, _} = Sources.share(F.actor(), grant.id, next, now)
      assert_receive {:evidence_read, _}
      assert used() == 1
    end)
  end

  test "changed intent and revoked membership cannot reuse a paid receipt for source work",
       context do
    with_core(context, fn ->
      now = System.system_time(:second)
      grant = F.provision(10, now)
      input = F.source_input(1, now)
      F.observe_reads()
      assert {:ok, _} = Sources.share(F.actor(), grant.id, input, now)
      assert_receive {:evidence_read, _}

      assert {:error, :idempotency_conflict} =
               Sources.share(F.actor(), grant.id, Map.put(input, "ttl", 301), now)

      refute_receive {:evidence_read, _}
      F.set_members([])
      assert {:error, :access_denied} = Sources.share(F.actor(), grant.id, input, now)
      refute_receive {:evidence_read, _}
      assert used() == 1
    end)
  end

  test "revoked source retry and unavailable journal refuse before reading or charging",
       context do
    with_core(context, fn ->
      now = System.system_time(:second)
      grant = F.provision(10, now)
      input = F.source_input(1, now)
      F.observe_reads()
      Application.put_env(:query_service_ex, :customer_query_operation_store, UnavailableJournal)
      assert {:error, :query_usage_unavailable} = Sources.share(F.actor(), grant.id, input, now)
      refute_receive {:evidence_read, _}
      assert used() == 0
      Application.put_env(:query_service_ex, :customer_query_operation_store, Journal)
      assert {:ok, shared} = Sources.share(F.actor(), grant.id, input, now)
      assert_receive {:evidence_read, _}
      assert :ok = Records.revoke(F.tenant(), "sources", shared.source["ref"])
      assert {:error, :source_denied} = Sources.share(F.actor(), grant.id, input, now)
      refute_receive {:evidence_read, _}
      assert used() == 1
    end)
  end

  test "membership revoked during admitted work prevents disclosure", context do
    with_core(context, fn ->
      now = System.system_time(:second)
      grant = F.provision(1, now)
      input = F.source_input(1, now)
      Application.put_env(:query_service_ex, :agent_run_source, RevokingSource)
      assert {:error, :access_denied} = Sources.share(F.actor(), grant.id, input, now)
      assert_receive {:evidence_read, _}
      assert used() == 1
    end)
  end

  test "unsupported or incomplete proposals consume no units and perform no reads", context do
    with_core(context, fn ->
      now = System.system_time(:second)
      grant = F.provision(10, now)
      input = F.prepare_input(grant, now)
      F.observe_reads()

      for kind <- ["run_comparison", "event_timeline"] do
        proposal = %{input["proposal"] | "kind" => kind, "sources" => []}

        assert {:error, :unsupported_evidence} =
                 Review.prepare(
                   grant.token,
                   F.binding(),
                   Map.put(input, "proposal", proposal),
                   now
                 )
      end

      refute_receive {:evidence_read, _}
      assert used() == 2
    end)
  end

  defp used do
    assert {:ok, %{"used" => used}} = Usage.snapshot(F.tenant())
    used
  end
end
