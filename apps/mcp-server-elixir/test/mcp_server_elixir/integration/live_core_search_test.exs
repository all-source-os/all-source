defmodule McpServerElixir.Integration.LiveCoreSearchTest do
  @moduledoc """
  The MCP search tools against a running Core process, over real HTTP.

  Needs a Core with auth disabled, so requests run as `dev-tenant`:

      docker run -d -p 39001:3900 -e ALLSOURCE_AUTH_DISABLED=true \\
        ghcr.io/all-source-os/allsource-core-community:latest
      LIVE_CORE_URL=http://localhost:39001 mix test --include integration \\
        test/mcp_server_elixir/integration/live_core_search_test.exs

  Set `LIVE_CORE_SEMANTIC=1` when that Core was built with an embedding model;
  otherwise semantic search must answer with the 503 refusal.
  """

  use ExUnit.Case, async: false

  @moduletag :integration

  alias McpServerElixir.Infrastructure.CoreClient

  @tenant "dev-tenant"

  setup_all do
    url =
      System.get_env("LIVE_CORE_URL") ||
        raise "LIVE_CORE_URL must name a running Core for the live search test"

    previous = Application.get_env(:mcp_server_elixir, :core_url)
    Application.put_env(:mcp_server_elixir, :core_url, url)

    on_exit(fn ->
      if previous,
        do: Application.put_env(:mcp_server_elixir, :core_url, previous),
        else: Application.delete_env(:mcp_server_elixir, :core_url)
    end)

    token = "quokka#{System.unique_integer([:positive])}"
    entity_id = "live-search-#{token}"

    {:ok, _} =
      CoreClient.ingest_event(%{
        "event_type" => "note.created",
        "entity_id" => entity_id,
        "tenant_id" => @tenant,
        "payload" => %{"text" => "#{token} crossed the river at dawn"}
      })

    {:ok, token: token, entity_id: entity_id}
  end

  test "hybrid search finds the ingested event", %{token: token, entity_id: entity_id} do
    {:ok, %{"results" => hits, "count" => count}} = await_hit(token, 20)

    assert count >= 1
    hit = Enum.find(hits, &(&1["entity_id"] == entity_id))
    assert hit, "#{entity_id} missing from #{inspect(hits)}"
    assert hit["event_type"] == "note.created"
    assert is_number(hit["keyword_score"])
  end

  test "semantic search answers for the model this Core was built with", %{token: token} do
    result = CoreClient.semantic_search(%{"query" => token, "limit" => 5})

    if System.get_env("LIVE_CORE_SEMANTIC") == "1" do
      assert {:ok, %{"results" => [_ | _]}} = result
    else
      assert {:error, message} = result
      assert message =~ "HTTP 503"
      assert message =~ "no embedding model"
    end
  end

  defp await_hit(_token, 0), do: flunk("hybrid search never returned the ingested event")

  defp await_hit(token, attempts) do
    case CoreClient.hybrid_search(%{"query" => token, "limit" => 10}) do
      {:ok, %{"count" => count}} = ok when count > 0 ->
        ok

      _not_yet ->
        Process.sleep(500)
        await_hit(token, attempts - 1)
    end
  end
end
