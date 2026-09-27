defmodule McpServerElixir.CustomerReviewHTTPTest do
  use ExUnit.Case, async: false
  import ExUnit.CaptureLog
  alias McpServerElixir.Infrastructure.CustomerReviewHTTP

  defmodule Fixture do
    import Plug.Conn
    def init(opts), do: opts

    def call(conn, opts) do
      send(opts[:owner], {:request, conn.request_path, get_req_header(conn, "authorization")})
      conn = put_resp_content_type(conn, "application/json")

      case conn.request_path do
        "/large" ->
          send_resp(conn, 200, Jason.encode!(%{data: %{evidence: String.duplicate("x", 49_000)}}))

        "/oversize" ->
          send_resp(conn, 200, String.duplicate("PRIVATE-UPSTREAM", 5_000))

        "/redirect" ->
          conn
          |> put_resp_header("location", opts[:url] <> "/private")
          |> send_resp(307, "PRIVATE-UPSTREAM")

        "/compressed" ->
          conn |> put_resp_header("content-encoding", "gzip") |> send_resp(200, "PRIVATE-UPSTREAM")

        "/chunked" ->
          conn = send_chunked(conn, 200)

          Enum.reduce_while(1..100, conn, fn _, conn ->
            case chunk(conn, String.duplicate("x", 4_096)) do
              {:ok, next} -> {:cont, next}
              {:error, _} -> {:halt, conn}
            end
          end)

        "/slow" ->
          # test-hang-allow: bounded delay deliberately exceeds the 30 ms client deadline.
          Process.sleep(100)
          send_resp(conn, 200, ~s({"data":{"state":"late"}}))

        "/malformed" ->
          send_resp(conn, 200, ~s({"data":{},"private":"PRIVATE-UPSTREAM"}))

        path ->
          send_resp(conn, String.trim_leading(path, "/") |> String.to_integer(), "PRIVATE-UPSTREAM")
      end
    end
  end

  setup do
    # test-hang-allow: supervised loopback fixture, request deadlines and shutdown bounded.
    server =
      start_supervised!(
        {Bandit,
         plug: {Fixture, owner: self(), url: "http://127.0.0.1:1"}, port: 0, ip: {127, 0, 0, 1}}
      )

    {:ok, {_, port}} = ThousandIsland.listener_info(server)
    %{url: "http://127.0.0.1:#{port}"}
  end

  test "48 KB evidence arrives intact; oversized, compressed, chunked and malformed replies stay private",
       %{url: url} do
    assert {:ok, %{"evidence" => evidence}} =
             CustomerReviewHTTP.post(url <> "/large", "synthetic-secret", "{}")

    assert byte_size(evidence) == 49_000

    log =
      capture_log(fn ->
        for path <- ~w(oversize compressed chunked malformed redirect) do
          assert {:error, :access_unavailable} =
                   CustomerReviewHTTP.post(url <> "/" <> path, "synthetic-secret", "{}")
        end
      end)

    refute log =~ "synthetic-secret"
    refute log =~ "PRIVATE-UPSTREAM"
    refute_receive {:request, "/private", _}
  end

  test "fixed status errors cannot echo bodies; client does not retry", %{url: url} do
    for {status, error} <- [
          {401, :access_denied},
          {403, :access_denied},
          {402, :query_quota_exceeded},
          {409, :review_conflict},
          {410, :review_expired},
          {422, :invalid_proposal},
          {429, :rate_limited},
          {503, :access_unavailable}
        ] do
      path = "/#{status}"
      assert {:error, ^error} = CustomerReviewHTTP.post(url <> path, "synthetic", "{}")
      assert_receive {:request, ^path, ["Bearer synthetic"]}
      refute_receive {:request, ^path, _}, 10
      refute_receive {:hackney_response, _, _}, 10
    end
  end

  test "total deadline and request cap terminate without a retry", %{url: url} do
    started = System.monotonic_time(:millisecond)

    assert {:error, :access_unavailable} =
             CustomerReviewHTTP.post(url <> "/slow", "synthetic", "{}", 30)

    assert System.monotonic_time(:millisecond) - started < 500
    assert_receive {:request, "/slow", _}
    refute_receive {:request, "/slow", _}, 10

    assert {:error, :access_unavailable} =
             CustomerReviewHTTP.post(url <> "/large", "synthetic", String.duplicate("x", 65_537))

    refute_receive {:request, "/large", _}, 10
  end
end
