defmodule QueryServiceEx.Application.Services.EventHistoryReads do
  @moduledoc "Authenticated host sessions, single-use evidence intents and metered source reads."
  alias QueryServiceEx.Application.Services.CustomerQueryAdmission
  alias QueryServiceEx.Application.Services.EventHistoryAccess, as: A
  alias QueryServiceEx.Domain.AgentRun.Event
  alias QueryServiceEx.Domain.EventHistory

  def session(c, token, params) when map_size(params) == 0 do
    with {:ok, p} <- A.principal(token, c) do
      A.update(p.subject, &add_session(&1, p.id))
    end
  end

  def session(c, token, %{"session_id" => id} = params) when map_size(params) == 1 do
    with {:ok, p} <- A.principal(token, c),
         {:ok, _} <- host_session(p, id),
         do: {:ok, %{session_id: id}}
  end

  def session(_, _, _), do: {:error, :invalid_request}

  defp add_session(reg, grant_id) do
    case reg[grant_id] do
      %{"status" => "active"} = grant ->
        active = Map.filter(grant["sessions"], fn {_, created} -> created + 3600 > A.now() end)

        if map_size(active) < 16 do
          id = uuid()
          grant = %{grant | "sessions" => Map.put(active, id, A.now())}
          {:ok, Map.put(reg, grant_id, grant), %{session_id: id}}
        else
          {:error, :capacity}
        end

      _ ->
        {:error, :invalid_token}
    end
  end

  def timeline(
        c,
        token,
        %{"session_id" => session, "request_id" => request, "arguments" => args} = params
      )
      when map_size(params) == 3 do
    with {:ok, p} <- A.principal(token, c),
         {:ok, started} <- host_session(p, session),
         true <- request_id?(request),
         clock = DateTime.utc_now(),
         {:ok, filter} <- EventHistory.select(args, clock) do
      tenant = p.workspace["id"]
      owner = owner(p.subject, tenant, p.grant["client_id"], c.resource, p.id)
      op = operation(started, [session, request])

      intent =
        if map_size(args) == 1,
          do: %{"entity" => filter["entity_id"], "window" => "previous_24h"},
          else: filter

      window_end = if map_size(args) == 1, do: DateTime.to_unix(clock, :microsecond)

      read = fn saved_end ->
        pinned_clock = if saved_end, do: DateTime.from_unix!(saved_end, :microsecond), else: clock

        with {:ok, filter} <- EventHistory.select(args, pinned_clock),
             {:ok, page} <- A.store().history(tenant, filter),
             do: EventHistory.timeline(page, p.workspace, filter, c.origin)
      end

      with {:ok, timeline} <-
             CustomerQueryAdmission.execute_once(
               owner,
               "history.timeline",
               op,
               intent,
               A.now(),
               window_end,
               read
             ),
           {:ok, fresh} <- A.principal(token, c),
           {:ok, _} <- host_session(fresh, session),
           do: {:ok, Map.put(timeline, "workspace", fresh.workspace)}
    else
      false -> {:error, :invalid_request}
      error -> error
    end
  end

  def timeline(_, _, _), do: {:error, :invalid_request}

  def evidence_intent(c, actor, session, %{"workspace_id" => tenant, "event_id" => id} = params)
      when map_size(params) == 2 do
    with true <- Event.uuid?(id),
         {:ok, _} <- A.workspace(actor["subject_id"], tenant, c),
         {:ok, intent} <-
           A.seal(
             "navigation",
             %{
               subject: actor["subject_id"],
               session: session,
               tenant: tenant,
               id: id,
               operation: "#{A.now()}:#{uuid()}"
             },
             300
           ) do
      {:ok, %{intent: intent}}
    else
      {:error, :storage_unavailable} = error -> error
      _ -> {:error, :not_found}
    end
  end

  def evidence_intent(_, _, _, _), do: {:error, :invalid_request}

  def evidence(
        c,
        actor,
        session,
        %{"workspace_id" => tenant, "event_id" => id, "intent" => token} = params
      )
      when map_size(params) == 3 do
    subject = actor["subject_id"]

    with {:ok, %{session: ^session, subject: ^subject, tenant: ^tenant, id: ^id, operation: op}} <-
           A.open("navigation", token, 300),
         {:ok, workspace} <- A.workspace(subject, tenant, c) do
      owner =
        owner(subject, tenant, "event-history-browser", c.resource, binary_part(session, 0, 32))

      read = fn _ ->
        with {:ok, page} <- A.store().history(tenant, %{"event_id" => id}),
             do: EventHistory.evidence(page, workspace, id, c.origin)
      end

      with {:ok, result} <-
             CustomerQueryAdmission.execute_once(
               owner,
               "history.evidence",
               op,
               %{"workspace" => tenant, "event" => id},
               A.now(),
               nil,
               read
             ),
           {:ok, fresh} <- A.workspace(subject, tenant, c),
           do: {:ok, Map.put(result, "workspace", fresh)},
           else: (
             {:error, :access_denied} -> {:error, :not_found}
             other -> other
           )
    else
      {:error, :storage_unavailable} = error -> error
      _ -> {:error, :not_found}
    end
  end

  def evidence(_, _, _, _), do: {:error, :invalid_request}

  defp host_session(p, id) do
    with true <- Event.uuid?(id),
         start when is_integer(start) <- p.grant["sessions"][id],
         true <- start <= A.now() and start + 3600 > A.now(),
         do: {:ok, start},
         else: (_ -> {:error, :invalid_session})
  end

  defp request_id?(id) when is_binary(id),
    do: byte_size(id) in 1..128 and String.valid?(id) and not Regex.match?(~r/[\p{C}]/u, id)

  defp request_id?(id), do: is_integer(id) and abs(id) <= 9_007_199_254_740_991

  defp owner(subject, tenant, client, resource, id),
    do: %{
      "subject_id" => subject,
      "tenant_id" => tenant,
      "client_id" => client,
      "resource" => resource,
      "grant_id" => id
    }

  defp uuid do
    <<a::32, b::16, _::4, c::12, _::2, d::14, e::48>> = :crypto.strong_rand_bytes(16)
    raw = Base.encode16(<<a::32, b::16, 4::4, c::12, 2::2, d::14, e::48>>, case: :lower)

    <<a::binary-size(8), b::binary-size(4), c::binary-size(4), d::binary-size(4), e::binary>> =
      raw

    Enum.join([a, b, c, d, e], "-")
  end

  defp operation(started, key) do
    <<a::binary-size(8), b::binary-size(4), c::binary-size(4), d::binary-size(4),
      e::binary-size(12), _::binary>> = A.digest(key)

    "#{started}:#{a}-#{b}-#{c}-#{d}-#{e}"
  end
end
