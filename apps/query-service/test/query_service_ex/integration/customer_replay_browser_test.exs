defmodule QueryServiceEx.Integration.CustomerReplayBrowserTest do
  use ExUnit.Case, async: false
  alias QueryServiceEx.Application.Services.CustomerReplayReview, as: Review
  alias QueryServiceEx.Domain.CustomerAgent.ConnectionConsent
  alias QueryServiceEx.Infrastructure.Adapters.CustomerAgentGrantStore, as: Grants
  alias QueryServiceEx.TestSupport.CustomerAgentCore, as: Core
  alias QueryServiceEx.TestSupport.CustomerHumanBrowser
  alias QueryServiceEx.TestSupport.CustomerRemoteHTTP, as: HTTP
  alias QueryServiceEx.TestSupport.CustomerReplayFixture, as: R
  alias QueryServiceEx.TestSupport.MeteredEvidenceFixture, as: F
  @moduletag :integration
  @moduletag timeout: 1_850_000
  @moduletag skip: System.get_env("ALLSOURCE_REPLAY_BROWSER") != "true"
  setup do: R.setup_context()

  test "opt-in real Core and product replay browser fixture", context do
    Core.with_core(context, fn ->
      now = System.system_time(:second)
      R.provision(now)

      assert {:ok, grant} =
               Grants.issue(
                 F.binding(),
                 ConnectionConsent.evidence_operations(),
                 %{"accepted" => true, "version" => ConnectionConsent.replay_version()},
                 now,
                 3_600
               )

      assert {:ok, _} =
               Review.prepare(
                 grant.token,
                 F.binding(),
                 R.prepare_input(grant, now, 1, "event-count", 3_600),
                 now
               )

      token =
        HTTP.session(%{
          "sub" => F.actor()["subject_id"],
          "tenant_id" => F.tenant(),
          "exp" => now + 3_600
        })

      # test-hang-allow: opt-in loopback fixture, bounded hold and owned process cleanup.
      start_supervised!(
        {Bandit,
         plug:
           {CustomerHumanBrowser,
            token: token, target_url: "http://127.0.0.1:4346/dashboard/tools/agent-reviews"},
         port: 4467,
         ip: {127, 0, 0, 1}},
        id: :replay_browser
      )

      IO.puts(
        "Synthetic replay fixture ready: http://127.0.0.1:4467/fixture; connection #{grant.id}"
      )

      wait(
        System.fetch_env!("ALLSOURCE_REPLAY_BROWSER_STOP"),
        System.monotonic_time(:millisecond) + 1_800_000,
        0
      )
    end)
  end

  defp wait(path, deadline, refresh_at) do
    cond do
      File.exists?(path) ->
        :ok

      System.monotonic_time(:millisecond) >= deadline ->
        flunk("Browser fixture deadline exceeded")

      System.system_time(:second) >= refresh_at ->
        Application.put_env(:query_service_ex, :core_api_key, "Bearer " <> Core.token("admin"))
        wait(path, deadline, System.system_time(:second) + 120)

      true ->
        # test-hang-allow: opt-in manual proof, bounded by deadline and explicit stop file.
        Process.sleep(500)
        wait(path, deadline, refresh_at)
    end
  end
end
