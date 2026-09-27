defmodule QueryServiceExWeb.CustomerConnectionsController do
  @moduledoc """
  Product-session connection settings. No generic JWT/dev-mode authentication.
  This consent surface cannot approve or execute a consequential agent proposal.
  """
  use Phoenix.Controller, formats: [:json]
  import Plug.Conn

  alias QueryServiceEx.Application.Services.CustomerConnections
  alias QueryServiceEx.Application.Services.CustomerEvidenceReview
  alias QueryServiceEx.Application.Services.CustomerEvidenceSources
  alias QueryServiceEx.Application.Services.CustomerHumanEvidence
  alias QueryServiceEx.RateLimiter
  alias QueryServiceExWeb.CustomerHumanSession

  def index(conn, params), do: dispatch(conn, params, :list)
  def create(conn, params), do: dispatch(conn, params, :create)
  def revoke(conn, params), do: dispatch(conn, params, :revoke)
  def share(conn, params), do: dispatch(conn, params, :share)
  def inspect_run(conn, params), do: dispatch(conn, params, :inspect_run)
  def workspace(conn, params), do: dispatch(conn, params, :workspace)
  def read_review(conn, params), do: dispatch(conn, params, :read_review)
  def revoke_source(conn, params), do: dispatch(conn, params, :revoke_source)

  defp dispatch(conn, params, operation) do
    with true <- Application.get_env(:query_service_ex, :customer_connections_enabled, false),
         true <-
           get_in(params, ["consent", "version"]) != "review-replay-v3" or
             Application.get_env(:query_service_ex, :customer_replay_enabled, false),
         true <-
           not evidence_request?(operation, params) or
             Application.get_env(:query_service_ex, :customer_evidence_enabled, false),
         true <- conn.query_string == "",
         {:allow, _} <- RateLimiter.check_rate("customer-connections:admission", :free),
         {:ok, actor} <- CustomerHumanSession.actor(conn),
         {:allow, _} <-
           RateLimiter.check_rate("customer-connections:" <> actor["tenant_id"], :free),
         {:ok, result} <- perform(operation, actor, params) do
      conn |> put_resp_header("cache-control", "no-store") |> json(%{data: result})
    else
      {:error, :connection_limit} ->
        error(conn, 429, "connection_limit")

      {:error, :storage_unavailable} ->
        error(conn, 503, "access_unavailable")

      {:error, code}
      when code in [:review_unavailable, :query_usage_unavailable, :clock_moved_backwards] ->
        error(conn, 503, "access_unavailable")

      {:error, code}
      when code in [:review_busy, :query_usage_busy, :query_operation_capacity, :workspace_limit] ->
        error(conn, 429, "review_busy")

      {:error, :query_quota_exceeded} ->
        error(conn, 402, "query_quota_exceeded")

      {:error, code}
      when code in [:idempotency_conflict, :query_operation_conflict, :query_period_changed] ->
        error(conn, 409, "review_conflict")

      {:error, :query_operation_expired} ->
        error(conn, 410, "review_expired")

      {:error, :invalid_source_request} ->
        error(conn, 422, "invalid_source")

      {:error, code} when code in [:invalid_request, :invalid_grant, :invalid_consent] ->
        error(conn, 400, "invalid_request")

      {:deny, _} ->
        error(conn, 429, "rate_limited")

      _ ->
        error(conn, 403, "access_denied")
    end
  rescue
    _ -> error(conn, 503, "access_unavailable")
  end

  defp perform(:list, actor, params) when map_size(params) == 0,
    do: CustomerConnections.list(actor, System.system_time(:second))

  defp perform(:create, actor, params),
    do: CustomerConnections.create(actor, params, System.system_time(:second))

  defp perform(:revoke, actor, %{"id" => id} = params) when map_size(params) == 1,
    do: CustomerConnections.revoke(actor, id, System.system_time(:second))

  defp perform(:share, actor, %{"connection_id" => id, "source" => input} = params)
       when map_size(params) == 2,
       do: CustomerEvidenceSources.share(actor, id, input, System.system_time(:second))

  defp perform(:inspect_run, actor, %{"connection_id" => id, "source" => input} = params)
       when map_size(params) == 2,
       do: CustomerHumanEvidence.inspect_run(actor, id, input, System.system_time(:second))

  defp perform(:workspace, actor, %{"connection_id" => id} = params)
       when map_size(params) == 1,
       do: CustomerHumanEvidence.workspace(actor, id, System.system_time(:second))

  defp perform(:revoke_source, actor, %{"connection_id" => id, "source_id" => source} = params)
       when map_size(params) == 2,
       do: CustomerHumanEvidence.revoke_source(actor, id, source, System.system_time(:second))

  defp perform(
         :read_review,
         actor,
         %{
           "connection_id" => connection,
           "id" => id,
           "version" => version,
           "request_id" => request
         } = params
       )
       when map_size(params) == 4,
       do:
         CustomerEvidenceReview.read_human(
           actor,
           connection,
           id,
           version,
           request,
           System.system_time(:second)
         )

  defp perform(_, _, _), do: {:error, :invalid_request}

  defp evidence_request?(:create, params),
    do: get_in(params, ["consent", "version"]) in ~w(review-evidence-v2 review-replay-v3)

  defp evidence_request?(operation, _),
    do: operation in [:share, :inspect_run, :workspace, :read_review, :revoke_source]

  defp error(conn, status, code),
    do:
      conn
      |> put_resp_header("cache-control", "no-store")
      |> put_status(status)
      |> json(%{error: %{code: code}})
end
