defmodule QueryServiceEx.Infrastructure.Adapters.ConsumerAccessProbeTest do
  use ExUnit.Case, async: true

  alias QueryServiceEx.Infrastructure.Adapters.ConsumerAccessProbe

  defp gateway(accepted) do
    fn conn ->
      authorization = conn |> Plug.Conn.get_req_header("authorization") |> List.first()

      cond do
        conn.request_path != "/api/v1/events/query" -> Plug.Conn.send_resp(conn, 404, "")
        authorization in accepted -> Req.Test.json(conn, %{events: []})
        true -> Plug.Conn.send_resp(conn, 401, "")
      end
    end
  end

  defp run(canaries, accepted) do
    ConsumerAccessProbe.run(canaries, base_url: "http://gateway.test", plug: gateway(accepted))
  end

  test "fails closed when no canary is configured" do
    result = ConsumerAccessProbe.run([], base_url: "http://gateway.test")

    refute result.ok
    assert result.kinds == []
  end

  @canaries [
    %{kind: "onboarding_api_key", token: "old"},
    %{kind: "managed_api_key", token: "managed"}
  ]

  test "passes when every credential kind reads" do
    result = run(@canaries, ["Bearer old", "Bearer managed"])

    assert result.ok
    assert Enum.map(result.kinds, & &1.status) == [200, 200]
  end

  test "fails when one credential kind is rejected" do
    result = run(@canaries, ["Bearer managed"])

    refute result.ok
    assert [rejected, accepted] = result.kinds
    assert %{kind: "onboarding_api_key", status: 401, ok: false} = rejected
    assert %{status: 200, ok: true} = accepted
  end

  test "parse keeps only complete entries and rejects malformed JSON" do
    raw = ~s([{"kind":"managed_api_key","token":"t"},{"kind":"","token":"x"},{"kind":"k"}])
    assert ConsumerAccessProbe.parse(raw) == [%{kind: "managed_api_key", token: "t"}]

    assert ConsumerAccessProbe.parse("not json") == []
    assert ConsumerAccessProbe.parse(nil) == []
  end

  test "an asserted client key canary is reported as skipped and never fails the gate" do
    raw =
      ~s([{"kind":"managed_api_key","token":"managed"},) <>
        ~s({"kind":"asserted_client_key","client_id":"c","tenant":"t","kid":"k","private_jwk":{"kty":"EC"}}])

    canaries = ConsumerAccessProbe.parse(raw)
    assert [%{kind: "managed_api_key"}, %{kind: "asserted_client_key", skip: _}] = canaries

    result = run(canaries, ["Bearer managed"])
    assert result.ok

    assert [%{status: 200}, %{kind: "asserted_client_key", ok: true, skipped: reason}] =
             result.kinds

    assert reason =~ "gateway"
  end

  test "only asserted canaries still means nothing was probed" do
    result = run([%{kind: "asserted_client_key", skip: "gateway only"}], [])

    refute result.ok
    assert [%{kind: "asserted_client_key", skipped: "gateway only"}] = result.kinds
  end
end
