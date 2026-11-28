defmodule QueryServiceEx.Infrastructure.Adapters.EventHistoryStore do
  @moduledoc "Leader-only history reads and revision-checked grant records."
  alias QueryServiceEx.Domain.AgentRun.Event
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOwner

  def candidates(subject) do
    case fetch(
           :get,
           "/api/v1/events/history/workspaces?" <> URI.encode_query(%{subject_id: subject})
         ) do
      {:ok, 200, %{"tenant_ids" => ids}} when is_list(ids) and length(ids) <= 512 ->
        if Enum.all?(ids, &Event.tenant?/1),
          do: {:ok, Enum.uniq(ids)},
          else: {:error, :storage_unavailable}

      _ ->
        {:error, :storage_unavailable}
    end
  end

  def history(tenant, selectors) do
    query = selectors |> Map.put("tenant_id", tenant) |> URI.encode_query()

    case fetch(:get, "/api/v1/events/history?" <> query) do
      {:ok, 200, page} when is_map(page) -> {:ok, page}
      _ -> {:error, :source_unavailable}
    end
  end

  def grants(subject) do
    key = key(subject)

    case fetch(:get, "/api/v1/config/" <> key) do
      {:ok, 404, _} ->
        {:ok, %{}, nil}

      {:ok, 200,
       %{
         "key" => ^key,
         "value" => %{"subject" => ^subject, "grants" => grants},
         "revision" => revision
       }}
      when is_map(grants) ->
        if Event.uuid?(revision),
          do: {:ok, grants, revision},
          else: {:error, :storage_unavailable}

      _ ->
        {:error, :storage_unavailable}
    end
  end

  def replace(subject, grants, revision) do
    key = key(subject)
    value = %{subject: subject, grants: grants}
    condition = if revision, do: %{kind: "revision", revision: revision}, else: %{kind: "absent"}

    if byte_size(Jason.encode!(value)) <= 60_000 do
      case fetch(:post, "/api/v1/config/conditional/set", %{
             key: key,
             condition: condition,
             changed_by: "event-history",
             value: value
           }) do
        {:ok, 200, %{"saved" => true, "key" => ^key, "revision" => next}} ->
          if Event.uuid?(next) and next != revision, do: :ok, else: {:error, :storage_unavailable}

        {:ok, 409, %{"error" => "Concurrency error: Configuration precondition failed"}} ->
          {:error, :conflict}

        _ ->
          {:error, :storage_unavailable}
      end
    else
      {:error, :capacity}
    end
  end

  defp key(subject), do: "event_history_v1.grants." <> ReviewOwner.digest(subject)

  defp fetch(method, path, body \\ nil) do
    url =
      Application.get_env(:query_service_ex, :core_write_url) ||
        Application.get_env(:query_service_ex, :core_url)

    token = Application.get_env(:query_service_ex, :core_api_key)

    with %URI{scheme: scheme, host: host, path: p, userinfo: nil, query: nil, fragment: nil}
         when scheme in ["https", "http"] and is_binary(host) and p in [nil, ""] <-
           is_binary(url) && URI.parse(url),
         true <- is_binary(token) and token != "" do
      opts = [
        method: method,
        url: url <> path,
        headers: [{"authorization", token}],
        retry: false,
        redirect: false,
        receive_timeout: 3_000,
        request_timeout: 5_000,
        compressed: false,
        raw: true,
        finch: [pool_timeout: 1_000, protocols: [:http1], conn_opts: [timeout: 2_000]],
        into: &chunk/2
      ]

      opts = if body, do: Keyword.put(opts, :json, body), else: opts

      decode_response(Req.request(opts))
    else
      _ -> {:error, :storage_unavailable}
    end
  rescue
    _ -> {:error, :storage_unavailable}
  end

  defp decode_response({:ok, %{status: 404}}), do: {:ok, 404, nil}

  defp decode_response({:ok, %{status: status, body: bytes}})
       when status in [200, 409] and is_binary(bytes) do
    with {:ok, result} <- Jason.decode(bytes), do: {:ok, status, result}
  end

  defp decode_response(_), do: {:error, :storage_unavailable}

  defp chunk({:data, bytes}, {req, %{body: acc} = resp})
       when is_binary(acc) and byte_size(acc) + byte_size(bytes) <= 1_048_576,
       do: {:cont, {req, %{resp | body: acc <> bytes}}}

  defp chunk(_, {req, resp}), do: {:halt, {req, %{resp | body: :too_large}}}
end
