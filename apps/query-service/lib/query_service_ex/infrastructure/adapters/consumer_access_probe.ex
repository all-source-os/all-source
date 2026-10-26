defmodule QueryServiceEx.Infrastructure.Adapters.ConsumerAccessProbe do
  @moduledoc """
  Sends one canary credential of every kind customers already hold through
  this machine's own listener, so the whole user path (auth plugs, key
  lifecycle, egress budget, Core read) is exercised. Fly's deploy waits on the
  top-level check that calls it, so a build that rejects a credential kind
  customers hold never receives traffic. See
  docs/runbooks/CONSUMER_ACCESS_GATE.md.
  """

  require Logger

  @env "CONSUMER_CANARY_CREDENTIALS"
  @probe_path "/api/v1/events/query"
  @probe_params [entity_id: "consumer-access-canary", limit: 1]
  @cache_key {__MODULE__, :result}
  # The endpoint is public; one request cannot fan out into more than one
  # probe round per window.
  @cache_ttl_ms 30_000
  @timeout_ms 10_000
  @rejected_reason "an existing credential kind cannot read through this machine"

  @type canary :: %{kind: String.t(), token: String.t()}

  @spec result() :: map()
  def result do
    now = System.monotonic_time(:millisecond)

    case :persistent_term.get(@cache_key, nil) do
      {at, cached} when now - at < @cache_ttl_ms ->
        cached

      _ ->
        fresh = run(canaries(), base_url: local_base_url())
        :persistent_term.put(@cache_key, {now, fresh})
        fresh
    end
  end

  @spec canaries() :: [canary()]
  def canaries, do: parse(System.get_env(@env))

  @spec parse(String.t() | nil) :: [canary()]
  def parse(nil), do: []
  def parse(""), do: []

  def parse(raw) do
    case Jason.decode(raw) do
      {:ok, entries} when is_list(entries) ->
        for %{"kind" => kind, "token" => token} <- entries,
            is_binary(kind) and kind != "" and is_binary(token) and token != "",
            do: %{kind: kind, token: token}

      _ ->
        Logger.error("consumer access: #{@env} is not a JSON array of {kind,token}")
        []
    end
  end

  @spec run([canary()], keyword()) :: map()
  def run([], _req_options) do
    %{ok: false, reason: "no canary credentials configured", checked_at: now(), kinds: []}
  end

  def run(canaries, req_options) do
    kinds =
      Enum.map(canaries, fn %{kind: kind, token: token} ->
        status = probe(token, req_options)

        if status != 200 do
          Logger.warning("consumer access: credential kind #{inspect(kind)} answered #{status}")
        end

        %{kind: kind, status: status, ok: status == 200}
      end)

    ok = Enum.all?(kinds, & &1.ok)

    %{
      ok: ok,
      reason: if(ok, do: nil, else: @rejected_reason),
      checked_at: now(),
      kinds: kinds
    }
  end

  # 0 means no response arrived.
  defp probe(token, req_options) do
    [
      url: @probe_path,
      params: @probe_params,
      auth: {:bearer, token},
      receive_timeout: @timeout_ms,
      retry: false
    ]
    |> Keyword.merge(req_options)
    |> Req.get()
    |> case do
      {:ok, %Req.Response{status: status}} -> status
      {:error, _reason} -> 0
    end
  end

  defp local_base_url, do: "http://127.0.0.1:" <> (System.get_env("PORT") || "3902")

  defp now, do: DateTime.utc_now() |> DateTime.to_iso8601()
end
