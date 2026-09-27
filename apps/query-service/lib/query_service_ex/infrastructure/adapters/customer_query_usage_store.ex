defmodule QueryServiceEx.Infrastructure.Adapters.CustomerQueryUsageStore do
  @moduledoc "Bounded leader-only canonical query admission. No automatic retry, redirect, reset or legacy fallback."
  @behaviour QueryServiceEx.Domain.CustomerAgent.QueryUsagePort
  alias QueryServiceEx.Domain.AgentRun.Event
  alias QueryServiceEx.Domain.CustomerAgent.QueryAdmission
  @max_bytes 2_048
  @denials %{
    {400, "invalid_query_usage_request"} => :invalid_query_usage_request,
    {402, "quota_exceeded"} => :query_quota_exceeded,
    {403, "inactive_tenant"} => :access_denied,
    {409, "operation_conflict"} => :query_operation_conflict,
    {409, "period_changed"} => :query_period_changed,
    {410, "expired_operation"} => :query_operation_expired,
    {429, "receipt_capacity"} => :query_usage_busy,
    {429, "query_usage_busy"} => :query_usage_busy
  }

  @impl true
  def snapshot(tenant) do
    if Event.tenant?(tenant) do
      with {:ok, body} <- request(:get, path(tenant), nil),
           do: QueryAdmission.snapshot(body)
    else
      {:error, :invalid_query_usage_request}
    end
  end

  @impl true
  def admit(tenant, input) do
    if Event.tenant?(tenant) and QueryAdmission.request?(input, System.system_time(:second)) do
      with {:ok, body} <- request(:post, path(tenant) <> "/admit", input),
           do: QueryAdmission.receipt(body, input, System.system_time(:second))
    else
      {:error, :invalid_query_usage_request}
    end
  end

  defp path(tenant), do: "/api/v1/tenants/" <> tenant <> "/usage/queries"

  defp request(method, path, body) do
    task =
      Task.async(fn ->
        try do
          perform(method, path, body)
        rescue
          _ -> {:error, :query_usage_unavailable}
        catch
          _, _ -> {:error, :query_usage_unavailable}
        end
      end)

    case Task.yield(task, 6_000) || Task.shutdown(task, :brutal_kill) do
      {:ok, result} -> result
      _ -> {:error, :query_usage_unavailable}
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
        {:ok, %{status: status, body: bytes}} when is_binary(bytes) -> decode(status, bytes)
        _ -> {:error, :query_usage_unavailable}
      end
    end
  end

  defp decode(status, bytes) do
    case {status, Jason.decode(bytes)} do
      {200, {:ok, body}} ->
        {:ok, body}

      {status, {:ok, %{"error" => error} = body}} when map_size(body) == 1 ->
        {:error, Map.get(@denials, {status, error}, :query_usage_unavailable)}

      _ ->
        {:error, :query_usage_unavailable}
    end
  end

  defp chunk({:data, bytes}, {request, %{status: status, body: body} = response})
       when status in [200, 400, 402, 403, 409, 410, 429] and is_binary(body) do
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
        {:error, :query_usage_unavailable}
    end
  end
end
