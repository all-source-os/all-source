defmodule QueryServiceEx.Infrastructure.Adapters.AgentRunStore do
  @moduledoc """
  Fixed leader access for one metadata-only run. Reads at most 2 MiB, uses
  HTTP/1 backpressure and a six-second total deadline; never follows redirects
  or retries. Generic event payloads are validated before any downstream use.
  """
  @behaviour QueryServiceEx.Domain.AgentRun.SourcePort
  @behaviour QueryServiceEx.Domain.AgentRun.AppendPort
  alias QueryServiceEx.Domain.AgentRun.Event
  alias QueryServiceEx.Domain.AgentRun.Timeline
  @max_bytes 2_097_152

  @impl true
  def events(tenant, run_id) do
    with true <- Event.tenant?(tenant) and Event.uuid?(run_id),
         {:ok, url} <- url(),
         {:ok, body} <- bounded(fn -> request(url, tenant, run_id) end),
         {:ok, events} <- complete(body) do
      {:ok, events}
    else
      {:error, :run_too_large} = error -> error
      _ -> {:error, :source_unavailable}
    end
  end

  @impl true
  def append(request) do
    with :ok <- append_shape(request),
         {:ok, url} <- url() do
      bounded(fn ->
        response =
          Req.post(url <> "/api/v1/events", [json: request] ++ options(4_096))

        case response do
          {:ok, %{status: 200, body: body}} when is_binary(body) -> acknowledgement(body, request)
          {:ok, %{status: 409}} -> {:error, :version_conflict}
          _ -> {:error, :append_uncertain}
        end
      end)
      |> append_result()
    else
      _ -> {:error, :append_uncertain}
    end
  end

  defp append_result({:ok, _} = result), do: result
  defp append_result({:error, :version_conflict} = error), do: error
  defp append_result(_), do: {:error, :append_uncertain}

  defp append_shape(request) when is_map(request) do
    with true <-
           Enum.sort(Map.keys(request)) ==
             ~w(entity_id event_type expected_version metadata payload tenant_id),
         true <- Event.tenant?(request["tenant_id"]),
         {:ok, payload} <- Event.validate(request["payload"]),
         true <- request["entity_id"] == Event.entity(request["tenant_id"], payload["run_id"]),
         true <- request["event_type"] == "agent_run.v1." <> payload["kind"],
         version when is_integer(version) and version in 0..999 <- request["expected_version"],
         %{"agent_run_command_sha256" => hash} = metadata when map_size(metadata) == 1 <-
           request["metadata"],
         true <-
           is_binary(hash) and byte_size(hash) == 64 and Regex.match?(~r/\A[0-9a-f]+\z/, hash) do
      :ok
    else
      _ -> :error
    end
  end

  defp append_shape(_), do: :error

  defp acknowledgement(body, request) do
    with {:ok, %{"event_id" => id, "version" => version, "timestamp" => timestamp} = ack} <-
           Jason.decode(body),
         true <- Enum.sort(Map.keys(ack)) == ~w(event_id timestamp version),
         true <- Event.uuid?(id) and version === request["expected_version"] + 1,
         true <- is_binary(timestamp) and byte_size(timestamp) <= 40,
         {:ok, _, _} <- DateTime.from_iso8601(timestamp) do
      {:ok, ack}
    else
      _ -> {:error, :append_uncertain}
    end
  end

  defp request(url, tenant, run_id) do
    response =
      Req.get(
        url <> "/api/v1/events/query",
        [
          params: [
            tenant_id: tenant,
            entity_id: Event.entity(tenant, run_id),
            limit: Timeline.limit() + 1
          ]
        ] ++
          options(@max_bytes)
      )

    case response do
      {:ok, %{status: 200, body: body}} when is_binary(body) -> Jason.decode(body)
      {:ok, %{body: :too_large}} -> {:error, :run_too_large}
      _ -> {:error, :source_unavailable}
    end
  end

  defp options(max_bytes) do
    headers =
      case Application.get_env(:query_service_ex, :core_api_key) do
        value when is_binary(value) and value != "" -> [{"authorization", value}]
        _ -> []
      end

    [
      headers: headers,
      redirect: false,
      retry: false,
      raw: true,
      compressed: false,
      receive_timeout: 3_000,
      request_timeout: 5_000,
      finch: [pool_timeout: 1_000, protocols: [:http1], conn_opts: [timeout: 2_000]],
      into: &chunk(&1, &2, max_bytes)
    ]
  end

  defp chunk({:data, data}, {request, %{status: 200, body: body} = response}, max_bytes)
       when is_binary(body) do
    if byte_size(body) + byte_size(data) <= max_bytes,
      do: {:cont, {request, %{response | body: body <> data}}},
      else: {:halt, {request, %{response | body: :too_large}}}
  end

  defp chunk(_, {request, response}, _), do: {:halt, {request, %{response | body: :unavailable}}}

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
end
