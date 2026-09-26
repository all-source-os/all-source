defmodule QueryServiceEx.Domain.CustomerAgent.ProposalTest do
  use ExUnit.Case, async: true

  alias QueryServiceEx.Domain.CustomerAgent.Proposal

  @fixtures Path.expand("../../../fixtures/customer_agent", __DIR__)

  test "missing evidence stays unknown; a valid request has no execution authority" do
    assert {:ok, proposal} =
             Proposal.decode(File.read!(Path.join(@fixtures, "missing_sources.json")))

    assert proposal.kind == "replay_plan"
    assert proposal.sources == []

    assert Proposal.unknowns(proposal) == [
             "missing_replay_analysis",
             "unresolved_source_authority"
           ]

    refute Map.has_key?(proposal, :approved)
  end

  test "round-trips bounded versioned references without raw evidence" do
    assert {:ok, proposal} = Proposal.decode(File.read!(Path.join(@fixtures, "comparison.json")))
    assert length(proposal.sources) == 2
    assert Proposal.unknowns(proposal) == ["unresolved_source_authority"]
    assert byte_size(Proposal.fingerprint(proposal)) == 64
  end

  test "fingerprint binds comparison direction, revision, digest and target" do
    input = fixture("comparison.json")
    assert {:ok, first} = Proposal.validate(input)
    assert {:ok, reordered} = Proposal.validate(Map.update!(input, "sources", &Enum.reverse/1))
    refute Proposal.fingerprint(first) == Proposal.fingerprint(reordered)

    for key <- ["revision", "sha256"] do
      value = if key == "revision", do: 2, else: String.duplicate("c", 64)

      changed =
        update_in(input["sources"], fn [source | rest] -> [Map.put(source, key, value) | rest] end)

      assert {:ok, next} = Proposal.validate(changed)
      refute Proposal.fingerprint(first) == Proposal.fingerprint(next)
    end

    replay = fixture("missing_sources.json")
    assert {:ok, one} = Proposal.validate(replay)
    assert {:ok, two} = Proposal.validate(Map.put(replay, "projection_name", "events-per-day"))
    refute Proposal.fingerprint(one) == Proposal.fingerprint(two)
  end

  test "rejects forged authority, arbitrary queries and nested payloads" do
    for key <- ~w(approved consent tenant_id user_id entitlement query result expires_at) do
      assert {:error, :invalid_shape} =
               Proposal.validate(Map.put(fixture("comparison.json"), key, true))
    end

    with_payload =
      update_in(fixture("comparison.json")["sources"], fn [source | rest] ->
        [Map.put(source, "payload", "private") | rest]
      end)

    assert {:error, :invalid_source} = Proposal.validate(with_payload)
  end

  test "rejects invalid revisions, digests, references and duplicate handles" do
    input = fixture("comparison.json")

    for {key, value} <- [
          {"revision", 0},
          {"revision", -1},
          {"revision", 1.5},
          {"revision", "1"},
          {"revision", 9_007_199_254_740_992},
          {"sha256", String.duplicate("A", 64)},
          {"sha256", "missing"},
          {"ref", "https://private.example/path"},
          {"ref", "../../private"},
          {"kind", "sql"}
        ] do
      changed =
        update_in(input["sources"], fn [source | rest] -> [Map.put(source, key, value) | rest] end)

      assert {:error, :invalid_source} = Proposal.validate(changed)
    end

    source = hd(input["sources"])

    assert {:error, :duplicate_source} =
             Proposal.validate(Map.put(input, "sources", [source, source]))
  end

  test "bounds input size and rejects malformed JSON and incompatible source kinds" do
    assert {:error, :input_too_large} = Proposal.decode(String.duplicate(" ", 65_537))
    assert {:error, :invalid_json} = Proposal.decode("{")
    assert {:error, :invalid_shape} = Proposal.decode("[]")
    assert {:error, :invalid_shape} = Proposal.validate(%{schema_version: 1})

    input = fixture("comparison.json")

    assert {:error, :invalid_source_count} =
             Proposal.validate(Map.put(input, "sources", List.duplicate(%{}, 33)))

    assert {:error, :incompatible_sources} =
             Proposal.validate(Map.put(input, "kind", "event_timeline"))

    assert {:error, :invalid_target} =
             Proposal.validate(Map.put(input, "projection_name", "event-count"))

    assert {:error, :unsupported_version} = Proposal.validate(Map.put(input, "schema_version", 2))
  end

  test "partial comparisons identify missing side, not a successful comparison" do
    input = Map.update!(fixture("comparison.json"), "sources", &Enum.take(&1, 1))
    assert {:ok, proposal} = Proposal.validate(input)
    assert "missing_comparison_source" in Proposal.unknowns(proposal)
  end

  defp fixture(name), do: @fixtures |> Path.join(name) |> File.read!() |> Jason.decode!()
end
