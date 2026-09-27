defmodule QueryServiceEx.Infrastructure.Adapters.CoreConfigTransport do
  @moduledoc "Bounded leader-only transport for internal conditional configuration adapters."
  @max_bytes 65_536

  def request(method, path, body) do
    task =
      Task.async(fn ->
        try do
          perform(method, path, body)
        rescue
          _ -> {:error, :storage_unavailable}
        catch
          _, _ -> {:error, :storage_unavailable}
        end
      end)

    case Task.yield(task, 6_000) || Task.shutdown(task, :brutal_kill) do
      {:ok, result} -> result
      _ -> {:error, :storage_unavailable}
    end
  end

  defp perform(method, path, body) do
    with {:ok, url, token} <- connection() do
      options = [
        method: method,
        url: url <> path,
        headers: [{"authorization", token}],
        redirect: false,
        retry: false,
        raw: true,
        compressed: false,
        receive_timeout: 3_000,
        request_timeout: 5_000,
        finch: [pool_timeout: 1_000, protocols: [:http1], conn_opts: [timeout: 2_000]],
        into: &chunk/2
      ]

      options = if method == :post, do: Keyword.put(options, :json, body), else: options

      case Req.request(options) do
        {:ok, %{status: 404}} ->
          {:ok, 404, nil}

        {:ok, %{status: status, body: bytes}} when status in [200, 409] and is_binary(bytes) ->
          case Jason.decode(bytes) do
            {:ok, value} -> {:ok, status, value}
            _ -> {:error, :storage_unavailable}
          end

        _ ->
          {:error, :storage_unavailable}
      end
    end
  end

  defp chunk({:data, bytes}, {request, %{status: status, body: body} = response})
       when status in [200, 409] and is_binary(body) do
    if byte_size(body) + byte_size(bytes) <= @max_bytes,
      do: {:cont, {request, %{response | body: body <> bytes}}},
      else: {:halt, {request, %{response | body: :too_large}}}
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
        {:error, :storage_unavailable}
    end
  end
end
