defmodule QueryServiceEx.Integration.AgentRunOrderingTest do
  use ExUnit.Case, async: false
  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient
  alias QueryServiceEx.TestSupport.CustomerAgentCore
  import QueryServiceEx.TestSupport.CustomerAgentCore, only: [with_core: 2]

  @moduletag :integration
  @moduletag timeout: 60_000
  @moduletag skip: is_nil(System.get_env("ALLSOURCE_CORE_BINARY"))
  @tenant "synthetic-run-ordering"
  @entity "synthetic-agent-run-1"

  setup do
    CustomerAgentCore.setup_context()
  end

  test "acknowledged event versions survive HTTP query and a hard restart", context do
    recorded =
      with_core(context, fn ->
        for previous <- 0..2 do
          assert {:ok, %{status: 200, body: acknowledgement}} = append(previous)
          assert acknowledgement["version"] == previous + 1
        end

        assert {:ok, %{status: 409}} = append(0)
        assert {:ok, body} = query(@tenant)
        assert body["entity_version"] == 3
        assert Enum.map(body["events"], & &1["version"]) == [1, 2, 3]
        assert {:ok, %{"events" => []}} = query("other-synthetic-tenant")
        body["events"]
      end)

    with_core(context, fn ->
      assert {:ok, recovered} = query(@tenant)
      assert recovered["events"] == recorded
      assert recovered["entity_version"] == 3
      assert {:ok, %{status: 200, body: %{"version" => 4}}} = append(3)
      assert {:ok, final} = query(@tenant)
      assert Enum.map(final["events"], & &1["version"]) == [1, 2, 3, 4]
    end)

    with_core(context, fn ->
      # Recovery may checkpoint into cold Parquet; do not prime it with a query.
      assert {:ok, %{status: 200, body: %{"version" => 5}}} = append(4)
      assert {:ok, final} = query(@tenant)
      assert Enum.map(final["events"], & &1["version"]) == [1, 2, 3, 4, 5]
    end)
  end

  defp append(previous) do
    Tesla.post(RustCoreClient.write_client(), "/api/v1/events", %{
      tenant_id: @tenant,
      entity_id: @entity,
      event_type: "agent_run.v1.evidence",
      payload: %{synthetic: true},
      expected_version: previous
    })
  end

  defp query(tenant),
    do:
      RustCoreClient.query_events_page(tenant, %{entity_id: @entity, limit: 10, order: "asc"},
        consistency: :strong
      )
end
