defmodule QueryServiceEx.Infrastructure.Adapters.CustomerReviewStore do
  @moduledoc "Bounded admin-only Core config access for review metadata. No retries, redirects or process-local store."
  @behaviour QueryServiceEx.Domain.CustomerAgent.ReviewStorePort
  alias QueryServiceEx.Domain.AgentRun.Event
  alias QueryServiceEx.Domain.CustomerAgent.ReviewOwner, as: Owner
  alias QueryServiceEx.Domain.CustomerAgent.ReviewWorkspace
  @max_bytes 65_536

  @impl true
  def load(tenant) do
    with {:ok, key} <- registry_key(tenant) do
      case request(:get, "/api/v1/config/" <> key, nil) do
        {:ok, 404, _} ->
          {:ok, ReviewWorkspace.empty(tenant), nil}

        {:ok, 200, %{"key" => ^key, "value" => value, "revision" => revision}} ->
          if Event.uuid?(revision) and ReviewWorkspace.valid?(value, tenant),
            do: {:ok, value, revision},
            else: {:error, :storage_unavailable}

        _ ->
          {:error, :storage_unavailable}
      end
    end
  end

  @impl true
  def replace(tenant, value, revision) do
    with {:ok, key} <- registry_key(tenant),
         true <- ReviewWorkspace.valid?(value, tenant),
         true <- is_nil(revision) or Event.uuid?(revision),
         true <- byte_size(Jason.encode!(value)) <= 60_000 do
      conditional(key, value, revision)
    else
      _ -> {:error, :storage_unavailable}
    end
  end

  @impl true
  def revoke(tenant, kind, id) do
    with {:ok, key} <- marker_key(tenant, kind, id) do
      case conditional(key, %{"revoked" => true}, nil) do
        :ok ->
          :ok

        {:error, :conflict} ->
          case active?(tenant, kind, id) do
            {:error, :revoked} -> :ok
            _ -> {:error, :storage_unavailable}
          end

        _ ->
          {:error, :storage_unavailable}
      end
    end
  end

  @impl true
  def active?(tenant, kind, id) do
    with {:ok, key} <- marker_key(tenant, kind, id) do
      case request(:get, "/api/v1/config/" <> key, nil) do
        {:ok, 404, _} -> :ok
        {:ok, 200, %{"key" => ^key}} -> {:error, :revoked}
        _ -> {:error, :storage_unavailable}
      end
    end
  end

  defp conditional(key, value, revision) do
    condition = if revision, do: %{kind: "revision", revision: revision}, else: %{kind: "absent"}
    body = %{key: key, value: value, condition: condition, changed_by: "customer-review-service"}

    case request(:post, "/api/v1/config/conditional/set", body) do
      {:ok, 200, %{"key" => ^key, "saved" => true, "revision" => next}} ->
        if Event.uuid?(next) and next != revision, do: :ok, else: {:error, :storage_unavailable}

      {:ok, 409, %{"error" => "Concurrency error: Configuration precondition failed"}} ->
        {:error, :conflict}

      _ ->
        {:error, :storage_unavailable}
    end
  end

  defp registry_key(tenant) do
    if Event.tenant?(tenant),
      do: {:ok, "customer_review_v1.workspace." <> Owner.digest(tenant)},
      else: {:error, :storage_unavailable}
  end

  defp marker_key(tenant, kind, id) do
    if Event.tenant?(tenant) and kind in ~w(sources reviews) and Owner.id?(id),
      do: {:ok, "customer_review_v1.revoked." <> Owner.digest([tenant, kind, id])},
      else: {:error, :storage_unavailable}
  end

  defp request(method, path, body) do
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
