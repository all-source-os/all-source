defmodule QueryServiceEx.TestSupport.MeteredEvidenceFixture do
  @moduledoc "Synthetic customer evidence and temporary Core wiring for canonical metering proof."
  import ExUnit.Assertions
  import ExUnit.Callbacks
  alias QueryServiceEx.Application.Services.AgentRunEvidence
  alias QueryServiceEx.Application.Services.AgentRunRecorder
  alias QueryServiceEx.Application.Services.CustomerEvidenceSources, as: Sources
  alias QueryServiceEx.Domain.CustomerAgent.ConnectionConsent
  alias QueryServiceEx.Infrastructure.Adapters.CustomerAgentGrantStore, as: Grants
  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient
  alias QueryServiceEx.TestSupport.AgentRunFixture, as: Run
  alias QueryServiceEx.TestSupport.CustomerAgentCore, as: Core
  @tenant "synthetic-metered-evidence"
  @subject "oauth:google:synthetic-meter-owner"
  @binding %{
    "tenant_id" => @tenant,
    "subject_id" => @subject,
    "client_id" => "claude-code",
    "resource" => "https://example.test/customer-review"
  }

  defmodule ObservedSource do
    @moduledoc false
    alias QueryServiceEx.Infrastructure.Adapters.AgentRunStore

    def events(tenant, run) do
      send(Application.fetch_env!(:query_service_ex, :meter_test_owner), {:evidence_read, run})
      AgentRunStore.events(tenant, run)
    end
  end

  def tenant, do: @tenant
  def binding, do: @binding
  def actor, do: Map.take(@binding, ~w(tenant_id subject_id))
  def operation(number, now), do: "#{now}:#{Run.uuid(number)}"

  def setup_context do
    context = Core.setup_context()

    keys = [
      :customer_review_resource,
      :customer_query_operation_store,
      :customer_query_usage_store,
      :agent_run_source,
      :meter_test_owner
    ]

    previous = Enum.map(keys, &{&1, Application.get_env(:query_service_ex, &1)})
    Application.put_env(:query_service_ex, :customer_review_resource, @binding["resource"])
    Application.put_env(:query_service_ex, :meter_test_owner, self())

    on_exit(fn ->
      Enum.each(previous, fn
        {key, nil} -> Application.delete_env(:query_service_ex, key)
        {key, value} -> Application.put_env(:query_service_ex, key, value)
      end)
    end)

    context
  end

  def observe_reads,
    do: Application.put_env(:query_service_ex, :agent_run_source, ObservedSource)

  def provision(limit, now, binding \\ @binding) do
    assert {:ok, %{status: 201}} =
             Tesla.post(RustCoreClient.write_client(), "/api/v1/tenants", %{
               id: @tenant,
               name: "Synthetic metered evidence",
               metadata: %{
                 subscription: %{tier: "indie", status: "active"},
                 quotas: %{mcp_scope: "read", queries_quota: limit, queries_used: 0}
               }
             })

    set_members([%{"user_id" => @subject, "role" => "member"}])

    assert {:ok, grant} =
             Grants.issue(
               binding,
               ConnectionConsent.evidence_operations(),
               %{"accepted" => true, "version" => ConnectionConsent.evidence_version()},
               now,
               600
             )

    grant
  end

  def source_input(number, now) do
    run_id = Run.uuid(number)

    last_id =
      Run.history(run_id, if(number == 1, do: "pass", else: "fail"))
      |> Enum.reduce(nil, fn event, previous ->
        assert {:ok, receipt} =
                 AgentRunRecorder.record(@tenant, run_id, %{
                   "operation_id" => Run.uuid(800 + event["version"]),
                   "expected_version" => event["version"] - 1,
                   "event" => Map.put(event["payload"], "causation_id", previous)
                 })

        receipt.event_id
      end)

    assert {:ok, run} = AgentRunEvidence.read(@tenant, run_id)
    assert List.last(run.events)["id"] == last_id

    %{
      "consent" => %{"accepted" => true, "version" => "selected-run-evidence-v1"},
      "operation_id" => operation(900 + number, now),
      "run_id" => run_id,
      "revision" => run.revision,
      "sha256" => run.digest,
      "ttl" => 300
    }
  end

  def prepare_input(grant, now) do
    refs =
      for number <- [1, 2] do
        assert {:ok, shared} = Sources.share(actor(), grant.id, source_input(number, now), now)
        shared.source
      end

    %{
      "expected_revision" => 0,
      "idempotency_key" => operation(999, now),
      "proposal" => %{
        "schema_version" => 1,
        "kind" => "run_comparison",
        "projection_name" => nil,
        "sources" => refs
      }
    }
  end

  def set_quota(limit),
    do:
      assert(
        {:ok, %{status: 200}} =
          Tesla.patch(RustCoreClient.write_client(), "/api/v1/tenants/#{@tenant}/metadata", %{
            quotas: %{queries_quota: limit}
          })
      )

  def set_members(members),
    do:
      assert(
        {:ok, %{status: 200}} =
          Tesla.post(RustCoreClient.write_client(), "/api/v1/config", %{
            key: "team:#{@tenant}:members",
            value: %{"schema_version" => 2, "members" => members, "invitations" => %{}},
            changed_by: "synthetic-meter-test"
          })
      )
end
