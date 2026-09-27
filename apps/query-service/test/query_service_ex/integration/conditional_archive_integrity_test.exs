defmodule QueryServiceEx.Integration.ConditionalArchiveIntegrityTest do
  use ExUnit.Case, async: false
  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient
  alias QueryServiceEx.TestSupport.CustomerAgentCore
  import QueryServiceEx.TestSupport.CustomerAgentCore, only: [with_core: 2]

  @moduletag :integration
  @moduletag timeout: 60_000
  @moduletag skip: is_nil(System.get_env("ALLSOURCE_CORE_BINARY"))
  @tenant "synthetic-archive-http"

  setup do
    context = CustomerAgentCore.setup_context()
    partition = Path.join([context.directory, "storage", @tenant, "2026-09"])
    File.mkdir_p!(partition)
    corrupt = Path.join(partition, "events-unreadable.parquet")
    File.write!(corrupt, "unknown synthetic archive")
    Map.put(context, :corrupt, corrupt)
  end

  test "incomplete archives deny conditional writes across query and restart", context do
    with_core(context, fn ->
      assert_refused()
      assert {:ok, %{status: 200}} = append("ordinary", nil)
      assert {:ok, %{"events" => []}} = query("conditional")
      assert_refused()
      assert {:ok, %{status: 200}} = Tesla.get(RustCoreClient.write_client(), "/health")
    end)

    with_core(context, fn ->
      assert_refused()
      assert {:ok, %{"events" => [_ordinary]}} = query("ordinary")
      assert {:ok, %{"events" => []}} = query("conditional")
      assert_refused()
    end)

    assert File.read!(context.corrupt) == "unknown synthetic archive"
  end

  defp assert_refused do
    assert {:ok, %{status: 500, body: body}} = append("conditional", 0)
    assert body["error"] =~ "Cannot verify conditional version from incomplete archive"
  end

  defp append(entity, expected) do
    input = %{
      tenant_id: @tenant,
      entity_id: entity,
      event_type: "synthetic.updated",
      payload: %{synthetic: true}
    }

    input = if is_nil(expected), do: input, else: Map.put(input, :expected_version, expected)
    Tesla.post(RustCoreClient.write_client(), "/api/v1/events", input)
  end

  defp query(entity),
    do:
      RustCoreClient.query_events_page(@tenant, %{entity_id: entity, limit: 10},
        consistency: :strong
      )
end
