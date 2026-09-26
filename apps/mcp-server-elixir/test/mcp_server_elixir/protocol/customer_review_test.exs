defmodule McpServerElixir.Protocol.CustomerReviewTest do
  use ExUnit.Case, async: true
  import ExUnit.CaptureIO
  alias McpServerElixir.Protocol.CustomerReview
  alias McpServerElixir.Server

  test "existing stdio server dispatches customer profile exclusively even with admin flags" do
    state = %{
      customer_review: true,
      read_only: false,
      system_admin: true,
      control_plane_enabled: true
    }

    request = %{jsonrpc: "2.0", id: 1, method: "tools/list"} |> Jason.encode!()

    output =
      capture_io(fn ->
        assert {:noreply, ^state} = Server.handle_info({:stdin_line, request}, state)
      end)

    assert %{"result" => %{"tools" => tools}} = Jason.decode!(output)

    assert Enum.map(tools, & &1["name"]) == [
             "allsource_review_context",
             "allsource_validate_review_proposal"
           ]

    assert Enum.all?(tools, &(&1["annotations"]["readOnlyHint"] == true))
    assert Enum.all?(tools, &is_map(&1["outputSchema"]))

    for name <- [
          "query_events",
          "ingest_event",
          "create_tenant",
          "approve_review",
          "run_fleet_recovery"
        ] do
      response = call("tools/call", %{"name" => name, "arguments" => %{}})
      assert response.error.code == -32_602
    end
  end

  test "malformed input stays bounded and never echoes request content" do
    marker = "private-fixture-marker"
    assert %{error: %{code: -32_700}} = CustomerReview.handle_line("{")
    assert %{error: %{code: -32_600}} = CustomerReview.handle_line("{}")

    for input <- [
          marker,
          Jason.encode!(%{jsonrpc: "2.0", method: "tools/call", id: [], params: marker}),
          String.duplicate(marker, 8_000)
        ] do
      response = CustomerReview.handle_line(input)
      assert is_map(response.error)
      refute Jason.encode!(response) =~ marker
    end

    for arguments <- [[], nil, "bad", %{"approved" => true}] do
      assert %{error: %{code: -32_602}} =
               call("tools/call", %{"name" => "allsource_review_context", "arguments" => arguments})
    end
  end

  test "notifications have no reply; resources and arbitrary methods cannot bypass tool whitelist" do
    assert nil ==
             CustomerReview.handle_line(
               Jason.encode!(%{jsonrpc: "2.0", method: "notifications/initialized"})
             )

    assert nil ==
             CustomerReview.handle_line(
               Jason.encode!(%{
                 jsonrpc: "2.0",
                 method: "notifications/cancelled",
                 params: %{requestId: 1}
               })
             )

    assert %{result: %{}} = call("ping", %{})

    for method <- ["resources/read", "resources/list", "prompts/get", "execute"] do
      assert %{error: %{code: -32_601}} = call(method, %{})
    end
  end

  defp call(method, params),
    do:
      CustomerReview.handle_line(
        Jason.encode!(%{jsonrpc: "2.0", id: 1, method: method, params: params})
      )
end
