defmodule QueryServiceEx.Infrastructure.Adapters.ReplayAnalysisStore do
  @moduledoc "Fixed, bounded replay sample; no arbitrary query or claim of complete retained history."
  alias QueryServiceEx.Domain.AgentRun.Event
  @limit 2_097_152

  def sample(tenant, at) do
    task = Task.async(fn -> perform(tenant, at) end)

    case Task.yield(task, 6_000) || Task.shutdown(task, :brutal_kill) do
      {:ok, result} -> result
      _ -> {:error, :source_unavailable}
    end
  end

  defp perform(tenant, at) do
    with true <- Event.tenant?(tenant),
         {:ok, _, 0} <- DateTime.from_iso8601(at),
         {:ok, url, token} <- connection(),
         {:ok, %{status: 200, body: bytes}} when is_binary(bytes) <-
           Req.get(url <> "/api/v1/events/query",
             headers: [{"authorization", token}],
             params: [tenant_id: tenant, limit: 1_000, offset: 0, order: "asc", as_of: at],
             redirect: false,
             retry: false,
             raw: true,
             compressed: false,
             receive_timeout: 3_000,
             request_timeout: 5_000,
             finch: [pool_timeout: 1_000, protocols: [:http1], conn_opts: [timeout: 2_000]],
             into: &chunk/2
           ),
         {:ok, %{"events" => events} = body} <- Jason.decode(bytes),
         true <- valid_page?(body, tenant, events) do
      {:ok, body}
    else
      _ -> {:error, :source_unavailable}
    end
  rescue
    _ -> {:error, :source_unavailable}
  catch
    _, _ -> {:error, :source_unavailable}
  end

  defp valid_page?(body, tenant, events) when is_list(events) and length(events) <= 1_000 do
    body["count"] === length(events) and
      (is_nil(body["total_count"]) or
         (is_integer(body["total_count"]) and body["total_count"] >= length(events))) and
      Enum.all?(events, fn event ->
        is_map(event) and event["tenant_id"] == tenant and Event.uuid?(event["id"]) and
          is_binary(event["entity_id"]) and is_binary(event["event_type"])
      end)
  end

  defp valid_page?(_, _, _), do: false

  defp chunk({:data, bytes}, {request, %{status: 200, body: body} = response})
       when is_binary(body) do
    if byte_size(body) + byte_size(bytes) <= @limit,
      do: {:cont, {request, %{response | body: body <> bytes}}},
      else: {:halt, {request, %{response | body: :unavailable}}}
  end

  defp chunk(_, {request, response}), do: {:halt, {request, %{response | body: :unavailable}}}

  defp connection do
    url =
      Application.get_env(:query_service_ex, :core_write_url) ||
        Application.get_env(:query_service_ex, :core_url)

    token = Application.get_env(:query_service_ex, :core_api_key)

    case is_binary(url) && URI.parse(url) do
      %URI{scheme: scheme, host: host, path: path, query: nil, fragment: nil, userinfo: nil}
      when scheme in ["http", "https"] and is_binary(host) and path in [nil, ""] and
             is_binary(token) and byte_size(token) > 0 ->
        {:ok, url, token}

      _ ->
        {:error, :source_unavailable}
    end
  end
end
