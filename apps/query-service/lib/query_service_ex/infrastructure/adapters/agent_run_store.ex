defmodule QueryServiceEx.Infrastructure.Adapters.AgentRunStore do
  @moduledoc """
  Fixed leader query for one metadata-only run. Streams at most 2 MiB, uses
  HTTP/1 backpressure and a six-second total deadline; never follows redirects
  or retries. Generic event payloads are validated before any downstream use.
  """
  @behaviour QueryServiceEx.Domain.AgentRun.SourcePort
  alias QueryServiceEx.Domain.AgentRun.Event
  alias QueryServiceEx.Domain.AgentRun.Timeline
  @max_bytes 2_097_152

  @impl true
  def events(tenant, run_id) do
    with true <- valid_tenant?(tenant) and Event.uuid?(run_id),
         {:ok, url} <- url(),
         {:ok, body} <- bounded(fn -> request(url, tenant, run_id) end),
         {:ok, events} <- complete(body) do
      {:ok, events}
    else
      {:error, :run_too_large} = error -> error
      _ -> {:error, :source_unavailable}
    end
  end

  defp request(url, tenant, run_id) do
    headers =
      case Application.get_env(:query_service_ex, :core_api_key) do
        value when is_binary(value) and value != "" -> [{"authorization", value}]
        _ -> []
      end

    response =
      Req.get(url <> "/api/v1/events/query",
        params: [
          tenant_id: tenant,
          entity_id: Event.entity(tenant, run_id),
          limit: Timeline.limit() + 1
        ],
        headers: headers,
        redirect: false,
        retry: false,
        raw: true,
        compressed: false,
        receive_timeout: 3_000,
        request_timeout: 5_000,
        finch: [pool_timeout: 1_000, protocols: [:http1], conn_opts: [timeout: 2_000]],
        into: &chunk/2
      )

    case response do
      {:ok, %{status: 200, body: body}} when is_binary(body) -> Jason.decode(body)
      {:ok, %{body: :too_large}} -> {:error, :run_too_large}
      _ -> {:error, :source_unavailable}
    end
  end

  defp chunk({:data, data}, {request, %{status: 200, body: body} = response})
       when is_binary(body) do
    if byte_size(body) + byte_size(data) <= @max_bytes,
      do: {:cont, {request, %{response | body: body <> data}}},
      else: {:halt, {request, %{response | body: :too_large}}}
  end

  defp chunk(_, {request, response}), do: {:halt, {request, %{response | body: :unavailable}}}

  defp complete(%{
         "events" => events,
         "count" => count,
         "total_count" => total,
         "has_more" => false
       })
       when is_list(events) and is_integer(count) and is_integer(total) and
              count == length(events) and total == count do
    if count <= Timeline.limit(), do: {:ok, events}, else: {:error, :run_too_large}
  end

  defp complete(%{"total_count" => total}) when is_integer(total) and total > 1_000,
    do: {:error, :run_too_large}

  defp complete(_), do: {:error, :source_unavailable}

  defp bounded(fun) do
    task =
      Task.async(fn ->
        try do
          fun.()
        rescue
          _ -> {:error, :source_unavailable}
        catch
          _, _ -> {:error, :source_unavailable}
        end
      end)

    case Task.yield(task, 6_000) || Task.shutdown(task, :brutal_kill) do
      {:ok, result} -> result
      _ -> {:error, :source_unavailable}
    end
  end

  defp url do
    value =
      Application.get_env(:query_service_ex, :core_write_url) ||
        Application.get_env(:query_service_ex, :core_url)

    case is_binary(value) && URI.parse(value) do
      %URI{scheme: scheme, host: host, path: path, query: nil, fragment: nil, userinfo: nil}
      when scheme in ["http", "https"] and is_binary(host) and path in [nil, ""] ->
        {:ok, value}

      _ ->
        {:error, :source_unavailable}
    end
  end

  defp valid_tenant?(value) when is_binary(value) and byte_size(value) in 1..128,
    do: Regex.match?(~r/\A[A-Za-z0-9_-]+\z/, value)

  defp valid_tenant?(_), do: false
end
