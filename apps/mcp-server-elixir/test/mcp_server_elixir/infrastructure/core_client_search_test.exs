defmodule McpServerElixir.Infrastructure.CoreClientSearchTest do
  @moduledoc """
  CoreClient against Core's event-search routes: the request it sends to
  `GET /api/v1/search/semantic` and `POST /api/v1/search/hybrid`, how it decodes
  their `{"results": [...], "count": n}` answers, and that every non-200 answer
  (503 engine unavailable, 400 bad input) is an error rather than an empty result.
  """
  use ExUnit.Case, async: false

  alias McpServerElixir.Infrastructure.CoreClient
  alias McpServerElixir.Protocol.McpTools

  setup do
    original_url = Application.get_env(:mcp_server_elixir, :core_url)
    on_exit(fn -> Application.put_env(:mcp_server_elixir, :core_url, original_url) end)
    :ok
  end

  # Answers every request with `{status, body}` and sends
  # `{:requested, method, path_with_query, body}` to the test process.
  defp start_core(status, body) do
    {:ok, listen} =
      :gen_tcp.listen(0, [:binary, packet: :raw, active: false, reuseaddr: true, ip: {127, 0, 0, 1}])

    {:ok, port} = :inet.port(listen)
    parent = self()
    payload = Jason.encode!(body)

    spawn_link(fn -> accept_loop(listen, status, payload, parent) end)
    Application.put_env(:mcp_server_elixir, :core_url, "http://127.0.0.1:#{port}")
    port
  end

  defp accept_loop(listen, status, payload, parent) do
    case :gen_tcp.accept(listen) do
      {:ok, sock} ->
        spawn(fn ->
          with {:ok, method, path, request_body} <- read_request(sock, "") do
            send(parent, {:requested, method, path, request_body})

            :gen_tcp.send(sock, [
              "HTTP/1.1 #{status} X\r\n",
              "content-type: application/json\r\n",
              "content-length: #{byte_size(payload)}\r\n\r\n",
              payload
            ])
          end

          :gen_tcp.close(sock)
        end)

        accept_loop(listen, status, payload, parent)

      _ ->
        :ok
    end
  end

  defp read_request(sock, acc) do
    with {:ok, data} <- :gen_tcp.recv(sock, 0, 5_000) do
      acc = acc <> data

      case String.split(acc, "\r\n\r\n", parts: 2) do
        [head, body] ->
          [request_line | headers] = String.split(head, "\r\n")
          [method, path | _] = String.split(request_line, " ")

          length =
            Enum.find_value(headers, 0, fn header ->
              case String.split(String.downcase(header), ":", parts: 2) do
                ["content-length", value] -> String.to_integer(String.trim(value))
                _ -> nil
              end
            end)

          if byte_size(body) >= length,
            do: {:ok, method, path, body},
            else: read_request(sock, acc)

        _ ->
          read_request(sock, acc)
      end
    end
  end

  defp last_request do
    assert_received {:requested, method, path, body}
    uri = URI.parse(path)
    decoded = if body == "", do: nil, else: Jason.decode!(body)
    {method, uri.path, URI.decode_query(uri.query || ""), decoded}
  end

  defp drain_requests do
    receive do
      {:requested, _, _, _} -> drain_requests()
    after
      0 -> :ok
    end
  end

  @semantic_hit %{
    "event_id" => "5f0c8a7e-3d0b-4a53-9a2e-1b5c9d1e2f30",
    "score" => 0.91,
    "event_type" => "payment.failed",
    "entity_id" => "order-42",
    "timestamp" => "2026-10-30T12:00:00Z"
  }

  @hybrid_hit Map.merge(@semantic_hit, %{"semantic_score" => 0.88, "keyword_score" => nil})

  defp state, do: %{backend: CoreClient, read_only: true, control_plane_enabled: false}

  describe "semantic_search/1" do
    test "sends query, limit, event_type and min_similarity and nothing else" do
      start_core(200, %{"results" => [], "count" => 0})

      assert {:ok, %{"results" => [], "count" => 0}} =
               CoreClient.semantic_search(%{
                 "query" => "failed card payments",
                 "limit" => 5,
                 "event_type" => "payment.failed",
                 "min_similarity" => 0.6,
                 "threshold" => 0.9,
                 "tenant_id" => "someone-else"
               })

      assert {"GET", "/api/v1/search/semantic", query, nil} = last_request()

      assert query == %{
               "query" => "failed card payments",
               "limit" => "5",
               "event_type" => "payment.failed",
               "min_similarity" => "0.6"
             }
    end

    test "omits optional parameters that are not given" do
      start_core(200, %{"results" => [], "count" => 0})

      assert {:ok, _} = CoreClient.semantic_search(%{"query" => "refunds", "event_type" => nil})
      assert {"GET", _, %{"query" => "refunds"} = query, nil} = last_request()
      assert map_size(query) == 1
    end

    test "decodes each hit to the five contract fields" do
      start_core(200, %{
        "results" => [Map.put(@semantic_hit, "payload", %{"secret" => true})],
        "count" => 1
      })

      assert {:ok, %{"results" => [hit], "count" => 1}} =
               CoreClient.semantic_search(%{"query" => "card declined"})

      assert hit == @semantic_hit
    end

    test "a 503 from an unavailable embedding engine is an error naming the cause" do
      start_core(503, %{"error" => "embedding engine unavailable"})

      assert {:error, message} = CoreClient.semantic_search(%{"query" => "card declined"})
      assert message =~ "unavailable"
      assert message =~ "HTTP 503"
      assert message =~ "embedding engine unavailable"
      drain_requests()
    end

    test "a 400 for bad input is an error carrying Core's message" do
      start_core(400, %{"error" => "limit must be positive"})

      assert {:error, message} = CoreClient.semantic_search(%{"query" => "x", "limit" => -1})
      assert message =~ "GET /api/v1/search/semantic returned HTTP 400"
      assert message =~ "limit must be positive"
    end

    test "a 200 without the results/count shape is an error, not an empty result" do
      start_core(200, %{"events" => []})

      assert {:error, message} = CoreClient.semantic_search(%{"query" => "x"})
      assert message =~ "unexpected body"
    end

    test "a hit missing a contract field is an error" do
      start_core(200, %{"results" => [Map.delete(@semantic_hit, "event_id")], "count" => 1})

      assert {:error, message} = CoreClient.semantic_search(%{"query" => "x"})
      assert message =~ "unexpected body"
    end

    test "a missing or blank query is refused without calling Core" do
      start_core(200, %{"results" => [], "count" => 0})

      assert {:error, "search query is required"} = CoreClient.semantic_search(%{})

      assert {:error, "search query must not be empty"} =
               CoreClient.semantic_search(%{"query" => " "})

      refute_received {:requested, _, _, _}
    end
  end

  describe "hybrid_search/1" do
    test "posts exactly the contract body" do
      start_core(200, %{"results" => [], "count" => 0})

      assert {:ok, %{"results" => [], "count" => 0}} =
               CoreClient.hybrid_search(%{
                 "query" => "timeout error",
                 "limit" => 10,
                 "event_type" => "job.failed",
                 "entity_id" => "job-7",
                 "semantic_weight" => 0.3,
                 "keyword_weight" => 0.7,
                 "keywords" => "ignored",
                 "tenant_id" => "someone-else"
               })

      assert {"POST", "/api/v1/search/hybrid", _query, body} = last_request()

      assert body == %{
               "query" => "timeout error",
               "limit" => 10,
               "event_type" => "job.failed",
               "entity_id" => "job-7",
               "semantic_weight" => 0.3,
               "keyword_weight" => 0.7
             }
    end

    test "decodes each hit with nullable per-strategy scores" do
      start_core(200, %{"results" => [Map.put(@hybrid_hit, "extra", 1)], "count" => 1})

      assert {:ok, %{"results" => [hit], "count" => 1}} =
               CoreClient.hybrid_search(%{"query" => "card declined"})

      assert hit == @hybrid_hit
      assert hit["keyword_score"] == nil
    end

    test "a non-numeric strategy score is an error" do
      start_core(200, %{
        "results" => [Map.put(@hybrid_hit, "semantic_score", "high")],
        "count" => 1
      })

      assert {:error, message} = CoreClient.hybrid_search(%{"query" => "x"})
      assert message =~ "unexpected body"
    end

    test "a 503 from an unavailable index is an error naming the cause" do
      start_core(503, %{"error" => "vector index unavailable"})

      assert {:error, message} = CoreClient.hybrid_search(%{"query" => "card declined"})
      assert message =~ "POST /api/v1/search/hybrid, HTTP 503"
      assert message =~ "vector index unavailable"
      drain_requests()
    end

    test "a 400 for bad input is an error carrying Core's message" do
      start_core(400, %{"error" => "semantic_weight out of range"})

      assert {:error, message} =
               CoreClient.hybrid_search(%{"query" => "x", "semantic_weight" => 4})

      assert message =~ "HTTP 400"
      assert message =~ "semantic_weight out of range"
    end
  end

  describe "the search tools over CoreClient" do
    test "semantic_search_events sends its threshold as min_similarity" do
      start_core(200, %{"results" => [@semantic_hit], "count" => 1})

      assert {:ok, %{content: [%{text: summary} | _]}} =
               McpTools.call_tool(
                 "semantic_search_events",
                 %{
                   "query" => "card declined",
                   "threshold" => 0.55,
                   "event_type" => "payment.failed"
                 },
                 state()
               )

      assert summary =~ "Found: 1"

      assert {"GET", _, query, nil} = last_request()
      assert query["min_similarity"] == "0.55"
      assert query["event_type"] == "payment.failed"
      refute Map.has_key?(query, "threshold")
    end

    test "semantic_search_events reports an unavailable engine as a tool error" do
      start_core(503, %{"error" => "embedding engine unavailable"})

      assert {:error, message} =
               McpTools.call_tool("semantic_search_events", %{"query" => "x"}, state())

      assert message =~ "embedding engine unavailable"
      drain_requests()
    end

    test "hybrid_search joins semantic_query and keywords and lifts filters" do
      start_core(200, %{"results" => [@hybrid_hit], "count" => 1})

      assert {:ok, %{content: [%{text: summary} | _]}} =
               McpTools.call_tool(
                 "hybrid_search",
                 %{
                   "semantic_query" => "payment problems",
                   "keywords" => "declined",
                   "filters" => %{"event_type" => "payment.failed", "entity_id" => "order-42"}
                 },
                 state()
               )

      assert summary =~ "Found: 1"

      assert {"POST", _, _, body} = last_request()

      assert body == %{
               "query" => "payment problems declined",
               "limit" => 100,
               "event_type" => "payment.failed",
               "entity_id" => "order-42"
             }
    end

    test "hybrid_search refuses a time filter Core cannot apply" do
      start_core(200, %{"results" => [], "count" => 0})

      assert {:error, message} =
               McpTools.call_tool(
                 "hybrid_search",
                 %{"query" => "x", "filters" => %{"time_from" => "2026-01-01T00:00:00Z"}},
                 state()
               )

      assert message =~ "time_from"
      refute_received {:requested, _, _, _}
    end

    test "hybrid_search without any query text is refused" do
      start_core(200, %{"results" => [], "count" => 0})

      assert {:error, message} = McpTools.call_tool("hybrid_search", %{}, state())
      assert message =~ "needs query text"
      refute_received {:requested, _, _, _}
    end

    test "hybrid_search reports an unavailable index as a tool error" do
      start_core(503, %{"error" => "vector index unavailable"})

      assert {:error, message} = McpTools.call_tool("hybrid_search", %{"query" => "x"}, state())
      assert message =~ "vector index unavailable"
      drain_requests()
    end
  end
end
