defmodule QueryServiceEx.Integration.AgentRunEvidenceTest do
  use ExUnit.Case, async: false
  alias QueryServiceEx.Application.Services.AgentRunEvidence, as: Evidence
  alias QueryServiceEx.Domain.AgentRun.Event
  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient
  alias QueryServiceEx.TestSupport.AgentRunFixture, as: F
  alias QueryServiceEx.TestSupport.CustomerAgentCore
  import QueryServiceEx.TestSupport.CustomerAgentCore, only: [with_core: 2]
  @moduletag :integration
  @moduletag timeout: 60_000
  @moduletag skip: is_nil(System.get_env("ALLSOURCE_CORE_BINARY"))

  setup do
    CustomerAgentCore.setup_context()
  end

  test "actual Core run fold and comparison survive hard restart without writes", context do
    report =
      with_core(context, fn ->
        append_history(F.uuid(1), "pass")
        append_history(F.uuid(2), "fail")
        assert {:ok, baseline} = Evidence.read(F.tenant(), F.uuid(1))
        assert baseline.revision == 7
        assert baseline.unknowns == []
        assert {:ok, page} = Evidence.page(baseline, baseline.digest, 0, 3)
        assert page.next_version == 3
        assert {:ok, report} = Evidence.compare(F.tenant(), F.uuid(1), F.uuid(2))
        assert report.state == "divergent"
        assert report.first_divergence.change_number == 1
        assert report.execution == "none"
        assert {:ok, unchanged} = Evidence.read(F.tenant(), F.uuid(1))
        assert unchanged == baseline
        assert {:error, :not_found} = Evidence.read("other-synthetic-tenant", F.uuid(1))
        assert {:error, :not_found} = Evidence.compare(F.tenant(), F.uuid(1), F.uuid(999))
        report
      end)

    with_core(context, fn ->
      assert {:ok, ^report} = Evidence.compare(F.tenant(), F.uuid(1), F.uuid(2))
      assert {:ok, body} = RustCoreClient.query_events_page(F.tenant(), %{limit: 100})
      assert body["total_count"] == 14
    end)
  end

  test "generic ingestion cannot smuggle private fields into typed evidence", context do
    with_core(context, fn ->
      input =
        F.payload("run.started", F.uuid(1), nil) |> Map.put("prompt", "SYNTHETIC-PRIVATE-TEXT")

      append(input, 0)
      assert {:error, :invalid_run_evidence} = Evidence.read(F.tenant(), F.uuid(1))
    end)
  end

  defp append_history(run_id, outcome) do
    Enum.reduce(F.history(run_id, outcome), nil, fn event, previous ->
      payload = Map.put(event["payload"], "causation_id", previous)
      ack = append(payload, event["version"] - 1)
      assert ack["version"] == event["version"]
      ack["event_id"]
    end)
  end

  defp append(payload, previous) do
    assert {:ok, %{status: 200, body: body}} =
             Tesla.post(RustCoreClient.write_client(), "/api/v1/events", %{
               tenant_id: F.tenant(),
               entity_id: Event.entity(F.tenant(), payload["run_id"]),
               event_type: "agent_run.v1." <> payload["kind"],
               payload: payload,
               expected_version: previous
             })

    body
  end
end
