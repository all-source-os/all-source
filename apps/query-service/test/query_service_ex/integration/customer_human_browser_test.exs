defmodule QueryServiceEx.Integration.CustomerHumanBrowserTest do
  use ExUnit.Case, async: false
  import QueryServiceEx.TestSupport.CustomerAgentCore, only: [with_core: 2]
  alias QueryServiceEx.Application.Services.CustomerEvidenceReview, as: Review
  alias QueryServiceEx.Application.Services.CustomerEvidenceSources, as: Sources
  alias QueryServiceEx.Infrastructure.Adapters.CustomerAgentGrantStore, as: Grants
  alias QueryServiceEx.TestSupport.CustomerAgentCore, as: Core
  alias QueryServiceEx.TestSupport.CustomerHumanBrowser
  alias QueryServiceEx.TestSupport.CustomerRemoteHTTP, as: HTTP
  alias QueryServiceEx.TestSupport.MeteredEvidenceFixture, as: F

  @moduletag :integration
  @moduletag timeout: 1_850_000
  @moduletag skip: System.get_env("ALLSOURCE_HUMAN_BROWSER") != "true"

  setup do
    HTTP.setup_context()
    context = F.setup_context()
    prior = Application.get_env(:query_service_ex, :customer_evidence_enabled)
    Application.put_env(:query_service_ex, :customer_evidence_enabled, true)

    on_exit(fn ->
      if is_nil(prior),
        do: Application.delete_env(:query_service_ex, :customer_evidence_enabled),
        else: Application.put_env(:query_service_ex, :customer_evidence_enabled, prior)
    end)

    context
  end

  test "opt-in real Core and Query Service for manual product browser proof", context do
    with_core(context, fn ->
      now = System.system_time(:second)
      F.provision(100, now)

      assert {:ok, grant} =
               Grants.issue(
                 F.binding(),
                 ~w(read_context validate_proposal prepare_proposal read_review read_result),
                 %{"accepted" => true, "version" => "review-evidence-v2"},
                 now,
                 3600
               )

      refs =
        for number <- [1, 2] do
          input = Map.put(F.source_input(number, now), "ttl", 3600)
          assert {:ok, source} = Sources.share(F.actor(), grant.id, input, now)
          source.source
        end

      F.source_input(3, now)

      assert {:ok, receipt} =
               Review.prepare(
                 grant.token,
                 F.binding(),
                 %{
                   "expected_revision" => 0,
                   "idempotency_key" => F.operation(999, now),
                   "proposal" => %{
                     "schema_version" => 1,
                     "kind" => "run_comparison",
                     "projection_name" => nil,
                     "sources" => refs
                   }
                 },
                 now
               )

      token =
        HTTP.session(%{
          "sub" => F.actor()["subject_id"],
          "tenant_id" => F.tenant(),
          "exp" => now + 3600
        })

      # test-hang-allow: explicit opt-in, owned loopback server; bounded hold and Core cleanup.
      start_supervised!(
        {Bandit, plug: {CustomerHumanBrowser, token: token}, port: 4465, ip: {127, 0, 0, 1}},
        id: :evidence_browser
      )

      IO.puts(
        "Synthetic browser fixture ready: http://127.0.0.1:4465/fixture; connection #{grant.id}; review #{receipt.id}"
      )

      stop = System.fetch_env!("ALLSOURCE_HUMAN_BROWSER_STOP")
      wait_for_stop(stop, System.monotonic_time(:millisecond) + 1_800_000, 0)
    end)
  end

  defp wait_for_stop(path, deadline, refresh_at) do
    cond do
      File.exists?(path) ->
        :ok

      System.monotonic_time(:millisecond) >= deadline ->
        flunk("Manual browser fixture reached its bounded deadline")

      System.system_time(:second) >= refresh_at ->
        # The shared Core test credential lasts five minutes; browser proof can take longer.
        token = Core.token("admin")
        Application.put_env(:query_service_ex, :core_api_key, "Bearer " <> token)
        wait_for_stop(path, deadline, System.system_time(:second) + 120)

      true ->
        # test-hang-allow: bounded manual verification window, explicit stop file.
        Process.sleep(500)
        wait_for_stop(path, deadline, refresh_at)
    end
  end
end
