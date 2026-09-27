defmodule QueryServiceEx.Domain.CustomerReviewWorkspaceTest do
  use ExUnit.Case, async: true
  alias QueryServiceEx.Domain.CustomerAgent.ConnectionConsent
  alias QueryServiceEx.Domain.CustomerAgent.EvidenceSource
  alias QueryServiceEx.Domain.CustomerAgent.PendingReview
  alias QueryServiceEx.Domain.CustomerAgent.ReviewWorkspace
  alias QueryServiceEx.TestSupport.AgentRunFixture, as: F

  @now 1_800_000_000
  @owner %{
    "tenant_id" => "synthetic-review",
    "subject_id" => "oauth:google:synthetic-owner",
    "client_id" => "claude-code",
    "resource" => "https://example.test/customer-review",
    "grant_id" => String.duplicate("a", 32),
    "grant_expires_at" => @now + 300
  }

  test "source pins owner, host, grant, run revision and digest; expiry cannot exceed grant" do
    assert {:ok, source} = source()
    assert source["expires_at"] == @now + 300
    assert EvidenceSource.valid?(source, @owner["tenant_id"])
    ref = EvidenceSource.reference(source)

    assert ref == %{
             "kind" => "run_evidence",
             "ref" => source["id"],
             "revision" => 7,
             "sha256" => F.hash(1)
           }

    assert :ok = EvidenceSource.authorize(source, @owner, ref, @now)

    for key <- ~w(tenant_id subject_id client_id resource grant_id) do
      assert {:error, :source_denied} =
               EvidenceSource.authorize(source, Map.put(@owner, key, "other"), ref, @now)
    end

    assert {:error, :source_expired} = EvidenceSource.authorize(source, @owner, ref, @now + 300)
    assert {:error, :source_denied} = EvidenceSource.authorize(source, @owner, ref, @now - 1)

    assert {:error, :source_changed} =
             EvidenceSource.authorize(source, @owner, Map.put(ref, "revision", 8), @now)
  end

  test "unknown fields and private source content never enter source records" do
    assert {:ok, source} = source()

    refute EvidenceSource.valid?(
             Map.put(source, "prompt", "SYNTHETIC PRIVATE"),
             @owner["tenant_id"]
           )

    refute EvidenceSource.valid?(
             Map.put(source, "locator", "https://private.invalid"),
             @owner["tenant_id"]
           )

    assert {:error, :invalid_source} =
             EvidenceSource.new(
               @owner,
               %{run_id: F.uuid(1), revision: 0, digest: F.hash(1)},
               "#{@now}:#{F.uuid(90)}",
               @now,
               300
             )
  end

  test "workspace insert is idempotent only for the same exact source and owner" do
    {:ok, source} = source()
    empty = ReviewWorkspace.empty(@owner["tenant_id"])
    assert {:ok, next} = ReviewWorkspace.insert(empty, "sources", source, @now)
    assert ReviewWorkspace.valid?(next, @owner["tenant_id"])
    assert {:ok, ^next} = ReviewWorkspace.insert(next, "sources", source, @now)
    changed = Map.put(source, "sha256", F.hash(2))

    assert {:error, :idempotency_conflict} =
             ReviewWorkspace.insert(next, "sources", changed, @now)

    assert {:error, :clock_moved_backwards} =
             ReviewWorkspace.insert(next, "sources", source, @now - 1)
  end

  test "existing metadata consent cannot authorize evidence operations" do
    assert {:error, :invalid_consent} =
             ConnectionConsent.receipt(
               "claude-code",
               ["prepare_proposal"],
               %{"accepted" => true, "version" => ConnectionConsent.version()},
               @now
             )

    assert {:ok, receipt} =
             ConnectionConsent.receipt(
               "claude-code",
               ConnectionConsent.operations(),
               %{"accepted" => true, "version" => ConnectionConsent.version()},
               @now
             )

    assert receipt["fields"] == ConnectionConsent.fields()
    refute "selected_run_metadata" in receipt["fields"]

    assert {:ok, v2} =
             ConnectionConsent.receipt(
               "claude-code",
               ConnectionConsent.evidence_operations(),
               %{"accepted" => true, "version" => ConnectionConsent.evidence_version()},
               @now
             )

    assert "selected_run_metadata" in v2["fields"]

    assert {:error, :invalid_consent} =
             ConnectionConsent.receipt(
               "claude-code",
               ["prepare_proposal"],
               %{
                 "accepted" => true,
                 "version" => ConnectionConsent.evidence_version(),
                 "approved" => true
               },
               @now
             )
  end

  test "pending review binds exact report and owner, and refuses manufactured approval" do
    {:ok, source} = source()
    reference = EvidenceSource.reference(source)

    proposal = %{
      "schema_version" => 1,
      "kind" => "run_comparison",
      "projection_name" => nil,
      "sources" => [reference, Map.put(reference, "ref", String.duplicate("b", 32))]
    }

    report = %{state: "inconclusive", execution: "none"}

    assert {:ok, review} =
             PendingReview.new(@owner, proposal, report, "#{@now}:#{F.uuid(91)}", @now, @now + 60)

    assert PendingReview.valid?(review, @owner["tenant_id"])
    assert PendingReview.digest(review, report) == review["digest"]

    refute PendingReview.digest(review, %{report | state: "same_recorded_evidence"}) ==
             review["digest"]

    refute PendingReview.valid?(Map.put(review, "state", "approved"), @owner["tenant_id"])
    refute PendingReview.valid?(Map.put(review, "approved", true), @owner["tenant_id"])

    assert {:error, :invalid_review} =
             PendingReview.new(
               @owner,
               proposal,
               report,
               "#{@now}:#{F.uuid(91)}",
               @now,
               @now + 301
             )
  end

  test "source issuance count is bounded across expired records during the rolling day" do
    full =
      Enum.reduce(1..64, ReviewWorkspace.empty(@owner["tenant_id"]), fn n, registry ->
        {:ok, record} =
          EvidenceSource.new(
            @owner,
            %{run_id: F.uuid(n), revision: 1, digest: F.hash(n)},
            "#{@now}:#{F.uuid(n)}",
            @now,
            1
          )

        {:ok, next} = ReviewWorkspace.insert(registry, "sources", record, @now)
        next
      end)

    {:ok, more} =
      EvidenceSource.new(
        @owner,
        %{run_id: F.uuid(65), revision: 1, digest: F.hash(65)},
        "#{@now}:#{F.uuid(65)}",
        @now + 2,
        60
      )

    assert {:error, :workspace_limit} = ReviewWorkspace.insert(full, "sources", more, @now + 2)
  end

  defp source,
    do:
      EvidenceSource.new(
        @owner,
        %{run_id: F.uuid(1), revision: 7, digest: F.hash(1)},
        "#{@now}:#{F.uuid(90)}",
        @now,
        600
      )
end
