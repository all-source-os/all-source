defmodule McpServerElixir.Infrastructure.CustomerReviewHTTP do
  @moduledoc "Bounded, non-retrying HTTP for the restricted customer profile."
  @limit 65_536
  @timeout 30_000

  def post(url, credential, body, timeout \\ @timeout)

  def post(url, credential, body, timeout)
      when byte_size(body) <= @limit and timeout in 1..@timeout do
    deadline = System.monotonic_time(:millisecond) + timeout

    headers = [
      {"authorization", "Bearer " <> credential},
      {"content-type", "application/json"},
      {"accept", "application/json"},
      {"accept-encoding", "identity"}
    ]

    options = [
      async: :once,
      pool: false,
      follow_redirect: false,
      protocols: [:http1],
      connect_timeout: min(5_000, timeout),
      recv_timeout: timeout
    ]

    case :hackney.request(:post, url, headers, body, options) do
      {:ok, ref} when is_pid(ref) ->
        try do
          receive_response(ref, deadline, nil, [], 0)
        after
          close(ref)
        end

      _ ->
        {:error, :access_unavailable}
    end
  rescue
    _ -> {:error, :access_unavailable}
  catch
    _, _ -> {:error, :access_unavailable}
  end

  def post(_, _, _, _), do: {:error, :access_unavailable}

  defp close(ref) do
    # Wait for the sender to terminate before flushing its queued headers/chunks.
    # Otherwise an error response can leave async messages in the stdio GenServer.
    monitor = Process.monitor(ref)
    Process.exit(ref, :kill)

    receive do
      {:DOWN, ^monitor, :process, ^ref, _} -> flush(ref)
    after
      1_000 -> Process.demonitor(monitor, [:flush])
    end
  end

  defp flush(ref) do
    receive do
      {:hackney_response, ^ref, _} -> flush(ref)
    after
      0 -> :ok
    end
  end

  defp receive_response(ref, deadline, status, chunks, size) do
    remaining = max(0, deadline - System.monotonic_time(:millisecond))

    if remaining == 0, do: throw(:deadline)

    receive do
      {:hackney_response, ^ref, {:status, code, _}} ->
        if code == 200,
          do: receive_response(ref, deadline, code, chunks, size),
          else: {:error, error(code)}

      {:hackney_response, ^ref, {:headers, headers}} ->
        if acceptable?(headers) do
          :hackney.stream_next(ref)
          receive_response(ref, deadline, status, chunks, size)
        else
          {:error, :access_unavailable}
        end

      {:hackney_response, ^ref, chunk}
      when is_binary(chunk) and size + byte_size(chunk) <= @limit ->
        :hackney.stream_next(ref)
        receive_response(ref, deadline, status, [chunk | chunks], size + byte_size(chunk))

      {:hackney_response, ^ref, :done} when status == 200 ->
        decode(chunks |> Enum.reverse() |> IO.iodata_to_binary())

      {:hackney_response, ^ref, _} ->
        {:error, :access_unavailable}
    after
      remaining -> {:error, :access_unavailable}
    end
  end

  defp acceptable?(headers) do
    headers =
      Enum.map(headers, fn {key, value} -> {String.downcase(to_string(key)), to_string(value)} end)

    # Hackney may buffer an entire transfer chunk before handing it to async_once.
    # The Query Service returns fixed-length JSON; reject other framing before reading.
    with [length] <- values(headers, "content-length"),
         {bytes, ""} <- Integer.parse(length),
         true <- bytes in 0..@limit,
         [] <- values(headers, "transfer-encoding"),
         encoding when encoding in [[], ["identity"]] <- values(headers, "content-encoding"),
         [type] <- values(headers, "content-type"),
         {:ok, "application", "json", _} <- Plug.Conn.Utils.content_type(type) do
      true
    else
      _ -> false
    end
  end

  defp values(headers, name), do: for({^name, value} <- headers, do: value)

  defp decode(body) do
    case Jason.decode(body) do
      {:ok, %{"data" => data} = envelope} when is_map(data) and map_size(envelope) == 1 ->
        {:ok, data}

      _ ->
        {:error, :access_unavailable}
    end
  end

  defp error(status) when status in [401, 403], do: :access_denied
  defp error(402), do: :query_quota_exceeded
  defp error(409), do: :review_conflict
  defp error(410), do: :review_expired
  defp error(422), do: :invalid_proposal
  defp error(429), do: :rate_limited
  defp error(_), do: :access_unavailable
end
