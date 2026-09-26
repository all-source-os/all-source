defmodule QueryServiceEx.Domain.CustomerAgent.ReviewStateTest do
  use ExUnit.Case, async: true

  alias QueryServiceEx.Domain.CustomerAgent.ReviewState

  @digest String.duplicate("a", 64)

  test "creates pending records with server-supplied ownership and bounded lifetime" do
    assert {:ok, state} = ReviewState.pending("tenant-1", "subject-1", @digest, 1_000, 60)
    assert state.owner_tenant == "tenant-1"
    assert state.owner_subject == "subject-1"
    assert state.authority_version == "allsource-operator-review-v1"
    assert ReviewState.status(state, @digest, 1_059) == :pending
    assert ReviewState.status(state, @digest, 1_060) == :expired

    for ttl <- [0, -1, 86_401, "60"] do
      assert {:error, :invalid_review} =
               ReviewState.pending("tenant-1", "subject-1", @digest, 1_000, ttl)
    end
  end

  test "rejects malformed ownership, digest and clocks" do
    assert {:error, :invalid_review} = ReviewState.pending("", "subject", @digest, 1, 60)
    assert {:error, :invalid_review} = ReviewState.pending("tenant", "", @digest, 1, 60)
    assert {:error, :invalid_review} = ReviewState.pending("tenant", "subject", "x", 1, 60)
    assert {:error, :invalid_review} = ReviewState.pending("tenant", "subject", @digest, -1, 60)
  end

  test "source changes supersede pending or previously approved records" do
    {:ok, state} = ReviewState.pending("tenant", "subject", @digest, 1_000, 60)
    other = String.duplicate("b", 64)
    assert ReviewState.status(state, other, 1_001) == :superseded
    assert ReviewState.status(%{state | decision: :approved}, other, 1_001) == :superseded
    assert ReviewState.status(%{state | decision: :approved}, @digest, 1_060) == :expired
    assert ReviewState.status(%{state | decision: :approved}, @digest, 1_001) == :approved
    assert ReviewState.status(state, nil, 1_001) == :unavailable
  end

  test "rejection is terminal; clock rollback and unknown authority fail closed" do
    {:ok, state} = ReviewState.pending("tenant", "subject", @digest, 1_000, 60)
    assert ReviewState.status(%{state | decision: :rejected}, @digest, 1_061) == :rejected
    assert ReviewState.status(state, @digest, 999) == :unavailable

    assert ReviewState.status(%{state | authority_version: "unknown"}, @digest, 1_001) ==
             :unavailable

    assert ReviewState.status(%{state | decision: :execute}, @digest, 1_001) == :unavailable
  end
end
