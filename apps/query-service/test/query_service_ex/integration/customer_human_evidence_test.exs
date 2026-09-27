defmodule QueryServiceEx.Integration.CustomerHumanEvidenceTest do
  use ExUnit.Case, async: false
  import QueryServiceEx.TestSupport.CustomerAgentCore, only: [with_core: 2]
  alias QueryServiceEx.Application.Services.CustomerEvidenceReview, as: Review
  alias QueryServiceEx.Infrastructure.Adapters.CustomerAgentGrantStore, as: Grants
  alias QueryServiceEx.Infrastructure.Adapters.CustomerQueryUsageStore, as: Usage
  alias QueryServiceEx.TestSupport.CustomerRemoteHTTP, as: HTTP
  alias QueryServiceEx.TestSupport.MeteredEvidenceFixture, as: F

  @moduletag :integration
  @moduletag timeout: 120_000
  @moduletag skip: is_nil(System.get_env("ALLSOURCE_CORE_BINARY"))

  setup do
    http = HTTP.setup_context()
    context = F.setup_context()
    previous = Application.get_env(:query_service_ex, :customer_evidence_enabled)
    Application.put_env(:query_service_ex, :customer_evidence_enabled, true)
    :ets.delete(:rate_limiter_buckets, "customer-connections:" <> F.tenant())

    on_exit(fn ->
      if is_nil(previous),
        do: Application.delete_env(:query_service_ex, :customer_evidence_enabled),
        else: Application.put_env(:query_service_ex, :customer_evidence_enabled, previous)

      :ets.delete(:rate_limiter_buckets, "customer-connections:" <> F.tenant())
    end)

    Map.put(context, :query_url, http.query_url)
  end

  test "inspection is metered but does not share; explicit shares and reviews recover from Core",
       context do
    {grant, review_request, view, saved} =
      with_core(context, fn ->
        now = System.system_time(:second)
        grant = F.provision(8, now)
        source = F.source_input(1, now)

        inspect = %{
          "connection_id" => grant.id,
          "source" => %{"run_id" => source["run_id"], "request_id" => F.operation(700, now)}
        }

        assert {200, %{"data" => inspected}, _} = post(context, "inspect-run", inspect)
        assert inspected["shared"] == false
        assert inspected["sha256"] == source["sha256"]
        refute Map.has_key?(inspected, "events")
        assert {200, %{"data" => ^inspected}, _} = post(context, "inspect-run", inspect)
        assert used() == 1

        assert {200, %{"data" => %{"sources" => [], "reviews" => []}}, _} =
                 workspace(context, grant)

        assert {200, %{"data" => shared}, _} =
                 post(context, "share", %{"connection_id" => grant.id, "source" => source})

        other = F.source_input(2, now)

        assert {200, %{"data" => second}, _} =
                 post(context, "share", %{"connection_id" => grant.id, "source" => other})

        input = %{
          "expected_revision" => 0,
          "idempotency_key" => F.operation(701, now),
          "proposal" => %{
            "schema_version" => 1,
            "kind" => "run_comparison",
            "projection_name" => nil,
            "sources" => [shared["source"], second["source"]]
          }
        }

        assert {:ok, receipt} = Review.prepare(grant.token, F.binding(), input, now)

        assert {:ok, agent} =
                 Review.read(grant.token, F.binding(), receipt.id, 1, F.operation(702, now), now)

        F.set_quota(9)

        request = %{
          "connection_id" => grant.id,
          "id" => receipt.id,
          "version" => 1,
          "request_id" => F.operation(703, now)
        }

        assert {200, %{"data" => human}, _} = post(context, "read-review", request)
        assert human == Jason.decode!(Jason.encode!(agent))
        assert human["evidence"]["state"] == "divergent"
        assert human["approved"] == false
        assert human["execution"] == "none"
        assert used() == 9
        assert {200, %{"data" => ^human}, _} = post(context, "read-review", request)
        assert used() == 9
        assert {200, %{"data" => saved}, _} = workspace(context, grant)
        assert length(saved["sources"]) == 2
        assert [%{"id" => id, "status" => "saved"}] = saved["reviews"]
        assert id == receipt.id
        refute inspect(saved) =~ "test_outcome"
        {grant, request, human, saved}
      end)

    with_core(context, fn ->
      assert {200, %{"data" => ^saved}, _} = workspace(context, grant)
      assert {200, %{"data" => ^view}, _} = post(context, "read-review", review_request)
      assert used() == 9
      source = hd(saved["sources"])["source"]["ref"]
      revoke = %{"connection_id" => grant.id, "source_id" => source}
      assert {200, %{"data" => %{"revoked" => true}}, _} = post(context, "revoke-source", revoke)
      assert {200, _, _} = post(context, "revoke-source", revoke)
      assert {200, %{"data" => hidden}, _} = post(context, "read-review", review_request)
      assert hidden["state"] == "unavailable"
      refute Map.has_key?(hidden, "evidence")
      assert {200, %{"data" => updated}, _} = workspace(context, grant)
      assert Enum.any?(updated["sources"], &(&1["status"] == "revoked"))
    end)
  end

  test "product routes reject agent credentials, foreign owners, forged scope and disabled features",
       context do
    with_core(context, fn ->
      now = System.system_time(:second)
      grant = F.provision(20, now)
      source = F.source_input(1, now)
      body = %{"connection_id" => grant.id}

      assert {403, _, _} =
               HTTP.http(context, :post, "connections/workspace", body, HTTP.bearer(grant.token))

      foreign = HTTP.session(%{"sub" => "oauth:google:foreign", "tenant_id" => F.tenant()})

      assert {403, _, _} =
               HTTP.http(context, :post, "connections/workspace", body, HTTP.bearer(foreign))

      assert {400, _, _} = post(context, "workspace", Map.put(body, "approved", true))
      assert {403, _, _} = post(context, "workspace?token=private", body)

      assert {200, %{"data" => shared}, _} =
               post(context, "share", Map.put(body, "source", source))

      assert {:ok, other} =
               Grants.issue(
                 F.binding(),
                 ~w(read_review prepare_proposal),
                 %{"accepted" => true, "version" => "review-evidence-v2"},
                 now,
                 600
               )

      assert {200, %{"data" => %{"sources" => [], "reviews" => []}}, _} =
               workspace(context, other)

      assert {403, _, _} =
               post(context, "revoke-source", %{
                 "connection_id" => other.id,
                 "source_id" => shared["source"]["ref"]
               })

      assert {:ok, metadata} =
               Grants.issue(
                 F.binding(),
                 ~w(read_context),
                 %{"accepted" => true, "version" => "review-metadata-v1"},
                 now,
                 600
               )

      assert {403, _, _} = workspace(context, metadata)
      Application.put_env(:query_service_ex, :customer_evidence_enabled, false)
      assert {403, _, _} = workspace(context, grant)

      assert {403, _, _} =
               post(context, "create", %{
                 "client_id" => "claude-code",
                 "ttl" => 600,
                 "operations" => ~w(read_review),
                 "consent" => %{"accepted" => true, "version" => "review-evidence-v2"}
               })

      Application.put_env(:query_service_ex, :customer_evidence_enabled, true)
      F.set_members([])
      assert {403, _, _} = workspace(context, grant)
      assert used() == 1
    end)
  end

  test "saved metadata and source revocation remain available after grant revocation",
       context do
    with_core(context, fn ->
      now = System.system_time(:second)
      grant = F.provision(1, now)
      source = F.source_input(1, now)

      assert {200, %{"data" => shared}, _} =
               post(context, "share", %{"connection_id" => grant.id, "source" => source})

      assert :ok = Grants.revoke(F.binding(), grant.id, now)
      assert {200, %{"data" => saved}, _} = workspace(context, grant)
      assert saved["connection_status"] == "revoked"
      assert hd(saved["sources"])["status"] == "unavailable"

      assert {200, _, _} =
               post(context, "revoke-source", %{
                 "connection_id" => grant.id,
                 "source_id" => shared["source"]["ref"]
               })

      assert {403, _, _} =
               post(context, "inspect-run", %{
                 "connection_id" => grant.id,
                 "source" => %{
                   "run_id" => source["run_id"],
                   "request_id" => F.operation(750, now)
                 }
               })

      assert used() == 1
    end)
  end

  defp post(context, operation, input),
    do:
      HTTP.http(
        context,
        :post,
        "connections/" <> operation,
        input,
        HTTP.bearer(HTTP.session(%{"sub" => F.actor()["subject_id"], "tenant_id" => F.tenant()}))
      )

  defp workspace(context, grant), do: post(context, "workspace", %{"connection_id" => grant.id})

  defp used do
    assert {:ok, %{"used" => used}} = Usage.snapshot(F.tenant())
    used
  end
end
