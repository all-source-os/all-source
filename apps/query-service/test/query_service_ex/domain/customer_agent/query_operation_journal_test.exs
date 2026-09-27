defmodule QueryServiceEx.Domain.CustomerAgent.QueryOperationJournalTest do
  use ExUnit.Case, async: true
  alias QueryServiceEx.Domain.CustomerAgent.QueryOperationJournal, as: Journal
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOperation, as: Operation
  alias QueryServiceEx.TestSupport.AgentRunFixture, as: F
  @now 1_800_000_000
  @owner %{
    "tenant_id" => "synthetic-query-journal",
    "subject_id" => "oauth:google:synthetic-owner",
    "client_id" => "claude-code",
    "resource" => "https://example.test/customer-review",
    "grant_id" => String.duplicate("a", 32)
  }

  test "one immutable operation binds owner, grant and purpose while changed intent conflicts" do
    base = request(1)
    stored = Map.put(base, "expected_period", 7)
    assert {:ok, journal} = Journal.insert(Journal.empty(@owner["tenant_id"]), stored, @now)
    assert {:ok, ^stored} = Journal.find(journal, base)
    changed = Map.put(base, "fingerprint", String.duplicate("b", 64))
    assert {:error, :idempotency_conflict} = Journal.find(journal, changed)
    assert {:error, :idempotency_conflict} = Journal.find(journal, Map.put(base, "count", 2))

    for key <- ~w(tenant_id subject_id client_id resource grant_id) do
      other = if key == "grant_id", do: String.duplicate("b", 32), else: @owner[key] <> "x"
      owner = Map.put(@owner, key, other)

      assert {:ok, distinct} =
               Operation.request(owner, "source.share", "#{@now}:#{F.uuid(1)}", %{}, 1, @now)

      assert distinct["operation_id"] != base["operation_id"]
    end

    assert {:ok, other_purpose} =
             Operation.request(@owner, "review.read", "#{@now}:#{F.uuid(1)}", %{}, 1, @now)

    assert other_purpose["operation_id"] != base["operation_id"]
  end

  test "capacity is bounded and expired IDs cannot be revived after pruning" do
    full =
      Enum.reduce(1..192, Journal.empty(@owner["tenant_id"]), fn n, journal ->
        assert {:ok, next} =
                 Journal.insert(journal, Map.put(request(n), "expected_period", 0), @now)

        next
      end)

    assert Journal.valid?(full, @owner["tenant_id"])
    assert byte_size(Jason.encode!(full)) < 60_000

    assert {:error, :query_operation_capacity} =
             Journal.insert(full, Map.put(request(193), "expected_period", 0), @now)

    next_time = @now + 3_600
    next_request = request(193, next_time) |> Map.put("expected_period", 1)
    assert {:ok, pruned} = Journal.insert(full, next_request, next_time)
    assert map_size(pruned["operations"]) == 1
    refute Operation.valid_at?("#{@now}:#{F.uuid(1)}", next_time)

    assert {:error, :invalid_operation} =
             Operation.request(@owner, "source.share", "#{@now}:#{F.uuid(1)}", %{}, 1, next_time)
  end

  test "invalid protocol metadata and clock reversal fail closed" do
    base = request(1)

    assert {:ok, journal} =
             Journal.insert(
               Journal.empty(@owner["tenant_id"]),
               Map.put(base, "expected_period", 0),
               @now
             )

    for change <- [nil, -1, 0.0, "0"] do
      invalid = put_in(journal, ["operations", base["operation_id"], "expected_period"], change)
      refute Journal.valid?(invalid, @owner["tenant_id"])
    end

    refute Journal.valid?(Map.put(journal, "approved", true), @owner["tenant_id"])
    refute Journal.valid?(journal, "other-tenant")

    assert {:error, :clock_moved_backwards} =
             Journal.insert(journal, Map.put(request(2), "expected_period", 0), @now - 1)

    refute Operation.valid_at?(F.uuid(1), @now)
    refute Operation.valid_at?("0#{@now}:#{F.uuid(1)}", @now)
    refute Operation.valid_at?("#{@now + 1}:#{F.uuid(1)}", @now)
  end

  defp request(number, now \\ @now) do
    assert {:ok, request} =
             Operation.request(@owner, "source.share", "#{now}:#{F.uuid(number)}", %{}, 1, now)

    request
  end
end
