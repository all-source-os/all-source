defmodule QueryServiceExWeb.EventHistoryController do
  @moduledoc "Dedicated Event History transport. Human and MCP credentials have disjoint verification paths."
  use Phoenix.Controller, formats: [:json]
  import Plug.Conn

  alias QueryServiceEx.Application.Services.{
    EventHistoryAccess,
    EventHistoryOAuth,
    EventHistoryReads
  }

  alias QueryServiceEx.RateLimiter
  alias QueryServiceExWeb.CustomerHumanSession

  for operation <- [
        :metadata,
        :resource,
        :token,
        :prepare,
        :inspect_request,
        :session,
        :timeline,
        :decision,
        :workspaces,
        :connections,
        :revoke,
        :evidence_intent,
        :evidence
      ] do
    def unquote(operation)(conn, params), do: handle(conn, params, unquote(operation))
  end

  defp handle(conn, params, op) do
    conn =
      conn
      |> put_resp_header("cache-control", "private, no-store")
      |> put_resp_header("referrer-policy", "no-referrer")

    with {:ok, config} <- EventHistoryAccess.config(),
         {:allow, _} <- RateLimiter.check_rate(rate_key(op, conn, config), :free),
         {:ok, result} <- bounded(fn -> dispatch(op, conn, params, config) end) do
      json(conn, if(op in [:timeline, :session], do: %{data: result}, else: result))
    else
      {:deny, _} -> failure(conn, :rate_limited)
      {:error, code} -> failure(conn, code)
      _ -> failure(conn, :invalid_request)
    end
  end

  defp rate_key(op, conn, config) do
    public = ["public", Tuple.to_list(conn.remote_ip)]

    cond do
      op in [:session, :timeline] ->
        with ["Bearer " <> token] <- get_req_header(conn, "authorization"),
             {:ok, %{"subject" => subject, "grant" => grant, "resource" => resource}} <-
               EventHistoryAccess.open("access", token, 3600),
             true <- resource == config.resource do
          ["host", subject, grant]
        else
          _ -> public
        end

      op in [:metadata, :resource, :token, :prepare, :inspect_request] ->
        public

      true ->
        case CustomerHumanSession.actor(conn) do
          {:ok, actor} -> ["browser", actor["subject_id"]]
          _ -> public
        end
    end
    |> then(&("event-history:" <> EventHistoryAccess.digest([op, &1])))
  end

  defp dispatch(:metadata, _, p, c) when map_size(p) == 0, do: EventHistoryOAuth.metadata(c)
  defp dispatch(:resource, _, p, c) when map_size(p) == 0, do: EventHistoryOAuth.resource(c)

  defp dispatch(:token, conn, p, c),
    do: EventHistoryOAuth.redeem(c, p, get_req_header(conn, "authorization"))

  defp dispatch(op, conn, p, c) when op in [:prepare, :inspect_request] do
    case get_req_header(conn, "authorization") do
      [] -> apply(EventHistoryOAuth, op, [c, p])
      _ -> {:error, :invalid_request}
    end
  end

  defp dispatch(op, conn, p, c) when op in [:session, :timeline] do
    case get_req_header(conn, "authorization") do
      ["Bearer " <> token] -> apply(EventHistoryReads, op, [c, token, p])
      _ -> {:error, :invalid_token}
    end
  end

  defp dispatch(op, conn, p, c) do
    with {:ok, actor} <- browser_actor(conn),
         ["Bearer " <> session] <- get_req_header(conn, "authorization") do
      case {op, p} do
        {:decision, _} ->
          EventHistoryOAuth.decision(c, actor, p)

        {:workspaces, params} when map_size(params) == 0 ->
          EventHistoryAccess.workspaces(actor, c)

        {:connections, params} when map_size(params) == 0 ->
          EventHistoryAccess.connections(actor, c)

        {:revoke, %{"workspace_id" => w, "id" => id}} when map_size(p) == 2 ->
          EventHistoryAccess.revoke(actor["subject_id"], id, w)

        {:evidence_intent, _} ->
          EventHistoryReads.evidence_intent(c, actor, EventHistoryAccess.digest(session), p)

        {:evidence, _} ->
          EventHistoryReads.evidence(c, actor, EventHistoryAccess.digest(session), p)

        _ ->
          {:error, :invalid_request}
      end
    end
  end

  defp browser_actor(conn) do
    case CustomerHumanSession.actor(conn) do
      {:ok, actor} -> {:ok, actor}
      _ -> {:error, :invalid_token}
    end
  end

  defp bounded(fun) do
    task =
      Task.async(fn ->
        try do
          fun.()
        rescue
          _ -> {:error, :storage_unavailable}
        catch
          _, _ -> {:error, :storage_unavailable}
        end
      end)

    case Task.yield(task, 15_000) || Task.shutdown(task, :brutal_kill) do
      {:ok, result} -> result
      _ -> {:error, :storage_unavailable}
    end
  end

  @failures %{
    invalid_token: {401, "invalid_token"},
    invalid_session: {404, "invalid_session"},
    invalid_grant: {400, "invalid_grant"},
    invalid_request: {400, "invalid_request"},
    access_denied: {403, "access_denied"},
    not_found: {404, "not_found"},
    disabled: {404, "unavailable"},
    query_quota_exceeded: {402, "query_quota_exceeded"},
    already_processed: {409, "already_processed"},
    idempotency_conflict: {409, "idempotency_conflict"},
    rate_limited: {429, "temporarily_unavailable"},
    capacity: {429, "temporarily_unavailable"},
    query_operation_capacity: {429, "temporarily_unavailable"}
  }

  defp failure(conn, code) do
    {status, message} = Map.get(@failures, code, {503, "temporarily_unavailable"})
    conn |> put_status(status) |> json(%{error: message})
  end
end
