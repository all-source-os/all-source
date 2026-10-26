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
end
