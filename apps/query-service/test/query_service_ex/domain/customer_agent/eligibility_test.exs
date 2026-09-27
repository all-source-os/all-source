defmodule QueryServiceEx.Domain.CustomerAgent.EligibilityTest do
  use ExUnit.Case, async: true

  alias QueryServiceEx.Domain.CustomerAgent.ConnectionGrant
  alias QueryServiceEx.Domain.CustomerAgent.Eligibility
  alias QueryServiceEx.Domain.CustomerAgent.ReviewState

  @now 1_800_000_000
  @binding %{
    "tenant_id" => "tenant-1",
    "subject_id" => "oauth:google:123456789",
    "client_id" => "claude-code",
    "resource" => "https://api.example.test/customer-review"
  }
  @members [%{"user_id" => @binding["subject_id"], "role" => "member"}]
  @tenant %{
    "id" => "tenant-1",
    "active" => true,
    "is_demo" => false,
    "metadata" => %{
      "subscription" => %{"status" => "active", "tier" => "indie"},
      "quotas" => %{"mcp_scope" => "read", "queries_quota" => 50_000, "queries_used" => 20}
    }
  }

  test "Control Plane OAuth subjects work in grants and pending reviews without relaxing path IDs" do
    assert {:ok, grant} = ConnectionGrant.new(@binding, ["read_context"], @now, 60)
    assert ConnectionGrant.valid_for?(grant, @binding, "read_context", @now)

    assert {:ok, review} =
             ReviewState.pending(
               "tenant-1",
               @binding["subject_id"],
               String.duplicate("a", 64),
               @now,
               60
             )

    assert ReviewState.status(review, review.digest, @now) == :pending

    for field <- ["tenant_id", "client_id"] do
      refute ConnectionGrant.valid_binding?(Map.put(@binding, field, "oauth:google:123"))
    end

    for subject <- ["", "../other", "alice\nadmin", String.duplicate("x", 257), <<255>>] do
      refute ConnectionGrant.valid_binding?(Map.put(@binding, "subject_id", subject))
    end
  end

  test "uses persisted MCP scope and live member role; returns no private billing or member fields" do
    assert {:ok, result} = Eligibility.check(@tenant, @members, @binding, @now)

    assert result == %{
             "membership_role" => "member",
             "mcp_scope" => "read",
             "entitlement_expires_at" => nil,
             "queries_remaining" => 49_980
           }

    # Tier labels do not confer access and retired labels do not discard the
    # current entitlement written by Control Plane's canonical billing rules.
    for tier <- ["indie", "starter", "pro", "growth", "custom"] do
      tenant = put_in(@tenant, ["metadata", "subscription", "tier"], tier)
      assert {:ok, _} = Eligibility.check(tenant, @members, @binding, @now)
    end
  end

  test "metered access preserves identity and entitlement after consuming the final unit" do
    exhausted = put_in(@tenant, ["metadata", "quotas", "queries_used"], 50_000)
    assert {:error, :access_denied} = Eligibility.check(exhausted, @members, @binding, @now)

    assert {:ok, %{"queries_remaining" => 0}} =
             Eligibility.check_metered(exhausted, @members, @binding, @now)

    assert {:error, :access_denied} = Eligibility.check_metered(exhausted, [], @binding, @now)
    inactive = put_in(exhausted, ["metadata", "subscription", "status"], "expired")

    assert {:error, :access_denied} =
             Eligibility.check_metered(inactive, @members, @binding, @now)

    for value <- [nil, -1, "50000", 50_000.0] do
      invalid = put_in(exhausted, ["metadata", "quotas", "queries_used"], value)

      assert {:error, :access_denied} =
               Eligibility.check_metered(invalid, @members, @binding, @now)
    end
  end

  test "unknown, duplicate, removed and service identities cannot borrow another member's access" do
    for members <- [
          [],
          [%{"user_id" => "someone-else", "role" => "admin"}],
          @members ++ @members,
          nil,
          [nil]
        ] do
      assert {:error, :access_denied} = Eligibility.check(@tenant, members, @binding, @now)
    end

    for role <- ["developer", "readonly", "serviceaccount", "viewer", nil] do
      assert {:error, :access_denied} =
               Eligibility.check(
                 @tenant,
                 [%{"user_id" => @binding["subject_id"], "role" => role}],
                 @binding,
                 @now
               )
    end

    assert {:ok, %{"membership_role" => "admin"}} =
             Eligibility.check(
               @tenant,
               [%{"user_id" => @binding["subject_id"], "role" => "admin"}],
               @binding,
               @now
             )
  end

  test "wrong, disabled, demo and incomplete tenant records never gain a default entitlement" do
    for tenant <- [
          nil,
          %{},
          Map.put(@tenant, "id", "other"),
          Map.put(@tenant, "active", false),
          Map.put(@tenant, "is_demo", true),
          Map.delete(@tenant, "active"),
          Map.delete(@tenant, "metadata")
        ] do
      assert {:error, :access_denied} = Eligibility.check(tenant, @members, @binding, @now)
    end

    for scope <- [nil, "", "write", "all"] do
      tenant = put_in(@tenant, ["metadata", "quotas", "mcp_scope"], scope)
      assert {:error, :access_denied} = Eligibility.check(tenant, @members, @binding, @now)
    end
  end

  test "keeps canonical dunning grace but denies canceled, expired and unknown subscription status" do
    for status <- ["active", "ACTIVE", "past_due"] do
      tenant = put_in(@tenant, ["metadata", "subscription", "status"], status)
      assert {:ok, _} = Eligibility.check(tenant, @members, @binding, @now)
    end

    for status <- [nil, "", "canceled", "expired", "unpaid", "paused", "unknown"] do
      tenant = put_in(@tenant, ["metadata", "subscription", "status"], status)
      assert {:error, :access_denied} = Eligibility.check(tenant, @members, @binding, @now)
    end
  end

  test "trial and subscription deadlines deny at exact expiry, including scheduler lag" do
    future = DateTime.from_unix!(@now + 60) |> DateTime.to_iso8601()

    trial =
      put_in(@tenant, ["metadata", "subscription"], %{
        "status" => "active",
        "tier" => "trial",
        "trial_expires_at" => future
      })

    assert {:ok, %{"entitlement_expires_at" => expiry}} =
             Eligibility.check(trial, @members, @binding, @now)

    assert expiry == @now + 60
    assert {:error, :access_denied} = Eligibility.check(trial, @members, @binding, @now + 60)

    for value <- [nil, "not-a-date", 42] do
      tenant = put_in(trial, ["metadata", "subscription", "trial_expires_at"], value)
      assert {:error, :access_denied} = Eligibility.check(tenant, @members, @binding, @now)
    end

    for status <- ["on_trial", "trialing"] do
      tenant = put_in(@tenant, ["metadata", "subscription", "status"], status)
      assert {:error, :access_denied} = Eligibility.check(tenant, @members, @binding, @now)
      tenant = put_in(tenant, ["metadata", "subscription", "trial_ends_at"], future)
      assert {:ok, _} = Eligibility.check(tenant, @members, @binding, @now)
    end

    ended = put_in(@tenant, ["metadata", "subscription", "subscription_ends_at"], future)
    assert {:error, :access_denied} = Eligibility.check(ended, @members, @binding, @now + 60)
  end

  test "paid conversion ignores historical trial dates without extending a live trial" do
    past = DateTime.from_unix!(@now - 60) |> DateTime.to_iso8601()
    paid = put_in(@tenant, ["metadata", "subscription", "trial_ends_at"], past)
    assert {:ok, _} = Eligibility.check(paid, @members, @binding, @now)
  end

  test "quota exhaustion and unknown usage deny; negotiated unlimited remains unlimited" do
    for {limit, used} <- [{0, 0}, {10, 10}, {10, 11}, {10, nil}, {10, -1}, {"10", 0}, {-2, 0}] do
      tenant =
        put_in(@tenant, ["metadata", "quotas"], %{
          "mcp_scope" => "read",
          "queries_quota" => limit,
          "queries_used" => used
        })

      assert {:error, :access_denied} = Eligibility.check(tenant, @members, @binding, @now)
    end

    tenant =
      put_in(@tenant, ["metadata", "quotas"], %{"mcp_scope" => "dedicated", "queries_quota" => -1})

    assert {:ok, %{"queries_remaining" => -1}} =
             Eligibility.check(tenant, @members, @binding, @now)
  end
end
