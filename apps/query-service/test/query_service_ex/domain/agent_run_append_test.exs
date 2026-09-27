defmodule QueryServiceEx.Domain.AgentRunAppendTest do
  use ExUnit.Case, async: true
  alias QueryServiceEx.Domain.AgentRun.AppendCommand
  alias QueryServiceEx.TestSupport.AgentRunFixture, as: F

  test "new commands bind the authoritative tenant, run, operation and predecessor" do
    assert {:ok, command} = command("run.started", 0, nil)
    assert {:append, request} = AppendCommand.prepare(command, [])
    assert request["tenant_id"] == F.tenant()
    assert request["expected_version"] == 0
    assert request["payload"]["kind"] == "run.started"
    assert Map.keys(request["metadata"]) == ["agent_run_command_sha256"]
    refute Jason.encode!(request) =~ F.uuid(900)

    started = F.history() |> Enum.take(1)
    assert {:ok, proposal} = command("change.proposed", 1, F.uuid(1001))
    assert {:append, _} = AppendCommand.prepare(proposal, started)
    assert {:ok, stale} = command("change.proposed", 0, F.uuid(1001))
    assert {:error, :stale_revision} = AppendCommand.prepare(stale, started)
    assert {:ok, wrong_cause} = command("change.proposed", 1, F.uuid(999))
    assert {:error, :stale_revision} = AppendCommand.prepare(wrong_cause, started)
  end

  test "same command recovers its acknowledgement without authorizing an external action" do
    assert {:ok, command} = command("run.started", 0, nil)
    assert {:append, request} = AppendCommand.prepare(command, [])
    event = F.stored(request["payload"], 1) |> Map.put("metadata", request["metadata"])
    assert {:existing, receipt} = AppendCommand.prepare(command, [event])
    assert receipt.event_id == event["id"]
    assert receipt.version == 1
    assert receipt.disposition == "already_recorded"
    assert receipt.execution == "none"
    assert receipt.action_authority == "not_established"

    changed = put_in(command.payload["prompt_sha256"], F.hash(44))
    assert {:error, :operation_conflict} = AppendCommand.prepare(changed, [event])
    revised = %{command | expected_version: 1}
    assert {:error, :operation_conflict} = AppendCommand.prepare(revised, [event])
  end

  test "invalid transitions, corrupt history and cross-tenant records fail before append" do
    assert {:ok, accepted} = command("attempt.accepted", 1, F.uuid(1001))

    assert {:error, :invalid_transition} =
             AppendCommand.prepare(accepted, Enum.take(F.history(), 1))

    assert {:error, :stale_revision} = AppendCommand.prepare(accepted, [])

    assert {:ok, proposal} = command("change.proposed", 1, F.uuid(1001))
    foreign = F.history() |> hd() |> Map.put("tenant_id", "different-tenant")
    assert {:error, :invalid_run_evidence} = AppendCommand.prepare(proposal, [foreign])
    corrupt = F.history() |> hd() |> Map.put("version", 2)
    assert {:error, :order_uncertain} = AppendCommand.prepare(proposal, [corrupt])
  end

  test "operation IDs are scoped to a tenant and run, never bearer credentials" do
    assert {:ok, first} = command("run.started", 0, nil)
    input = input("run.started", 0, nil)
    assert {:ok, other_tenant} = AppendCommand.new("other-tenant", F.uuid(1), input)
    other_input = put_in(input["event"]["run_id"], F.uuid(2))
    assert {:ok, other_run} = AppendCommand.new(F.tenant(), F.uuid(2), other_input)
    assert first.command_sha256 != other_tenant.command_sha256
    assert first.command_sha256 != other_run.command_sha256
  end

  test "closed input schema rejects private fields, forged binding and invalid revisions" do
    input = input("run.started", 0, nil)

    invalid = [
      nil,
      Map.put(input, "tenant_id", "other-tenant"),
      Map.put(input, "operation_id", "secret-not-an-id"),
      Map.put(input, "expected_version", 0.0),
      Map.put(input, "expected_version", -1),
      Map.put(input, "expected_version", 1000),
      put_in(input["event"]["run_id"], F.uuid(2)),
      put_in(input["event"]["prompt"], "SYNTHETIC PRIVATE")
    ]

    for request <- invalid do
      assert {:error, :invalid_append_command} = AppendCommand.new(F.tenant(), F.uuid(1), request)
    end

    assert {:error, :invalid_append_command} =
             AppendCommand.new("?tenant=other", F.uuid(1), input)
  end

  defp command(kind, version, previous),
    do: AppendCommand.new(F.tenant(), F.uuid(1), input(kind, version, previous))

  defp input(kind, version, previous),
    do: %{
      "operation_id" => F.uuid(900),
      "expected_version" => version,
      "event" => F.payload(kind, F.uuid(1), previous)
    }
end
