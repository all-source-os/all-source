defmodule McpServerElixir.Protocol.CustomerEvidenceTest do
  use ExUnit.Case, async: false
  alias McpServerElixir.Protocol.CustomerReview

  setup do
    previous = Application.get_env(:mcp_server_elixir, :customer_evidence_review)
    Application.put_env(:mcp_server_elixir, :customer_evidence_review, true)

    on_exit(fn ->
      if is_nil(previous),
        do: Application.delete_env(:mcp_server_elixir, :customer_evidence_review),
        else: Application.put_env(:mcp_server_elixir, :customer_evidence_review, previous)
    end)
  end

  test "only gated evidence tools are discovered; metered operations disclose writes" do
    tools = CustomerReview.tools()
    assert length(tools) == 5

    for tool <- Enum.drop(tools, 2) do
      refute tool.annotations.readOnlyHint
      assert tool.annotations.idempotentHint
      refute tool.annotations.destructiveHint
      refute tool.annotations.openWorldHint
    end

    Application.put_env(:mcp_server_elixir, :customer_evidence_review, false)
    assert length(CustomerReview.tools()) == 2

    assert %{error: %{code: -32_602}} =
             call("allsource_prepare_review", input(), fn _, _ ->
               flunk("Disabled tool reached network")
             end)
  end

  test "strict preparation schema rejects extra authority, source kinds and malformed retry identities before network" do
    input = input()

    for bad <- [
          Map.put(input, "approved", true),
          Map.put(input, "expected_revision", 1),
          Map.put(input, "binding", %{}),
          Map.put(input, "idempotency_key", "regenerate-me"),
          put_in(input, ["proposal", "kind"], "replay_plan"),
          put_in(input, ["proposal", "sources"], []),
          put_in(input, ["proposal", "schema_version"], 2)
        ] do
      assert %{error: %{code: -32_602}} =
               call("allsource_prepare_review", bad, fn _, _ ->
                 flunk("Invalid input reached network")
               end)
    end
  end

  test "preparation has matching structured and text pending receipts" do
    expected = Map.put(receipt(), "unknowns", [])
    input = input()
    response = call("allsource_prepare_review", input, fn "prepare", ^input -> {:ok, expected} end)
    assert response.result.isError == false
    assert response.result.structuredContent == expected
    assert Jason.decode!(hd(response.result.content).text) == expected
  end

  test "upstream approval claims and unknown fields never pass as successful data" do
    for result <- [
          Map.put(receipt(), "approved", true),
          Map.put(receipt(), "result_available", true),
          Map.put(receipt(), "private", "PRIVATE-UPSTREAM")
        ] do
      response =
        call("allsource_get_review_result", read_input(), fn "result", _ -> {:ok, result} end)

      assert response.result.isError
      refute Map.has_key?(response.result, :structuredContent)
      refute Jason.encode!(response) =~ "PRIVATE-UPSTREAM"
      assert hd(response.result.content).text =~ "may have persisted"
    end
  end

  test "expired status has no evidence and pending result cannot claim a delivered outcome" do
    expected = Map.put(receipt(), "state", "expired")
    response = call("allsource_get_review", read_input(), fn "review", _ -> {:ok, expected} end)
    refute response.result.isError
    assert response.result.structuredContent == expected
    pending = Map.put(receipt(), "result_available", false)

    response =
      call("allsource_get_review_result", read_input(), fn "result", _ -> {:ok, pending} end)

    refute response.result.isError
    refute response.result.structuredContent["approved"]
  end

  defp input do
    source = %{
      "kind" => "run_evidence",
      "ref" => String.duplicate("a", 32),
      "revision" => 1,
      "sha256" => String.duplicate("b", 64)
    }

    %{
      "expected_revision" => 0,
      "idempotency_key" => operation(),
      "proposal" => %{
        "schema_version" => 1,
        "kind" => "run_comparison",
        "projection_name" => nil,
        "sources" => [source, source]
      }
    }
  end

  defp operation, do: "1790535000:00000000-0000-0000-0000-000000000001"

  defp read_input,
    do: %{"id" => String.duplicate("c", 32), "version" => 1, "request_id" => operation()}

  defp receipt,
    do: %{
      "schema_version" => 1,
      "id" => String.duplicate("c", 32),
      "version" => 1,
      "digest" => String.duplicate("d", 64),
      "state" => "pending",
      "expires_at" => 1_790_538_600,
      "view_schema" => "run-comparison-v1",
      "persisted" => true,
      "approved" => false,
      "execution" => "none",
      "human_approval" => "required_in_product"
    }

  defp call(name, args, caller),
    do:
      CustomerReview.handle_line(
        Jason.encode!(%{
          jsonrpc: "2.0",
          id: 1,
          method: "tools/call",
          params: %{name: name, arguments: args}
        }),
        caller
      )
end
