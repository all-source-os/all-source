defmodule QueryServiceEx.Infrastructure.Adapters.CoreHold do
  @moduledoc """
  Tesla middleware that holds a Core request through a short Core restart.

  Any request is resent while no Core handler can have run: the connection was
  never made, or Core answered `503 {"error":"not_ready"}`. Core ingest has no
  idempotency key, so a write is never resent after a reset, a timeout or any
  other 5xx. Reads (GET, HEAD, OPTIONS) are also resent on any other error and
  on 408, 429 and 5xx, at most `:max_retries` times.

  No attempt starts after `@deadline_ms` from the first one, the same bound as
  the control plane's `corehold.Deadline`. It must sit outside
  `Tesla.Middleware.Timeout` so the timeout bounds each attempt, and outside
  `Tesla.Middleware.JSON` so every attempt re-encodes the original body.
  """

  @behaviour Tesla.Middleware

  @deadline_ms 6_000
  @first_delay_ms 50
  @max_delay_ms 500
  @unreached [:econnrefused, :nxdomain, :connect_timeout, :ehostunreach, :enetunreach]
  @idempotent [:get, :head, :options]
  @read_retry_statuses [408, 429, 500, 502, 503, 504]

  # 50ms doubling to 500ms over 12 retries waits about @deadline_ms in total.
  @req_max_retries 12

  @doc "Milliseconds after the first attempt past which no attempt starts."
  def deadline_ms, do: @deadline_ms

  @doc """
  Req options with the same rule for the stores that call Core through Req:
  resend only while no Core handler can have run.
  """
  def req_options do
    [retry: &req_retry?/2, max_retries: @req_max_retries, retry_delay: &req_delay/1]
  end

  defp req_retry?(_request, %Req.TransportError{reason: reason}), do: reason in @unreached
  defp req_retry?(_request, %Req.Response{status: 503, body: body}), do: not_ready?(body)
  defp req_retry?(_request, _response), do: false

  defp req_delay(count), do: min(@first_delay_ms * Integer.pow(2, count), @max_delay_ms)

  @impl Tesla.Middleware
  def call(env, next, opts) do
    read_budget = if env.method in @idempotent, do: Keyword.get(opts, :max_retries, 3), else: 0

    attempt(env, next, %{
      deadline: now() + Keyword.get(opts, :deadline_ms, @deadline_ms),
      read_budget: read_budget,
      delay: @first_delay_ms
    })
  end

  defp attempt(env, next, state) do
    result = Tesla.run(env, next)

    case classify(result) do
      :hold -> retry(result, env, next, state)
      :read when state.read_budget > 0 -> retry(result, env, next, spend(state))
      _ -> result
    end
  end

  defp retry(result, env, next, %{deadline: deadline, delay: delay} = state) do
    if now() + delay >= deadline do
      result
    else
      Process.sleep(delay)
      attempt(env, next, %{state | delay: min(delay * 2, @max_delay_ms)})
    end
  end

  defp spend(state), do: %{state | read_budget: state.read_budget - 1}

  defp classify({:error, reason}) when reason in @unreached, do: :hold
  defp classify({:error, _reason}), do: :read

  defp classify({:ok, %Tesla.Env{status: 503, body: body}}),
    do: if(not_ready?(body), do: :hold, else: :read)

  defp classify({:ok, %Tesla.Env{status: status}}) when status in @read_retry_statuses, do: :read
  defp classify({:ok, _env}), do: :done

  defp not_ready?(%{"error" => "not_ready"}), do: true

  defp not_ready?(body) when is_binary(body) do
    match?({:ok, %{"error" => "not_ready"}}, Jason.decode(body))
  end

  defp not_ready?(_body), do: false

  defp now, do: System.monotonic_time(:millisecond)
end
