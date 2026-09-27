defmodule QueryServiceExWeb.CustomerReplayController do
  @moduledoc "Default-off product replay decisions; requires session plus request-bound product relay proof."
  use Phoenix.Controller, formats: [:json]
  import Plug.Conn
  alias QueryServiceEx.Application.Services.CustomerReplayReview, as: Review
  alias QueryServiceEx.Application.Services.CustomerReplaySources, as: Sources
  alias QueryServiceEx.RateLimiter
  alias QueryServiceExWeb.CustomerHumanAction

  def inspect_source(conn, params), do: dispatch(conn, params, "inspect")
  def share(conn, params), do: dispatch(conn, params, "share")
  def prepare(conn, params), do: dispatch(conn, params, "prepare")
  def read(conn, params), do: dispatch(conn, params, "read")
  def edit(conn, params), do: dispatch(conn, params, "edit")
  def approve(conn, params), do: dispatch(conn, params, "approve")
  def reject(conn, params), do: dispatch(conn, params, "reject")
  def workspace(conn, params), do: dispatch(conn, params, "workspace")

  defp dispatch(conn, params, operation) do
    with true <- enabled?() and conn.query_string == "",
         {:allow, _} <- RateLimiter.check_rate("customer-replay:admission", :free),
         {:ok, actor} <- CustomerHumanAction.actor(conn, operation, params),
         true <- byte_size(Jason.encode!(params)) <= 16_384,
         {:allow, _} <- RateLimiter.check_rate("customer-replay:" <> actor["tenant_id"], :free),
         {:ok, result} <- perform(actor, operation, params, System.system_time(:second)) do
      conn |> put_resp_header("cache-control", "no-store") |> json(%{data: result})
    else
      {:deny, _} -> error(conn, 429, "rate_limited")
      {:error, code} -> failure(conn, code)
      _ -> error(conn, 403, "access_denied")
    end
  rescue
    _ -> error(conn, 503, "access_unavailable")
  end

  defp perform(actor, "workspace", %{"connection_id" => id} = params, now)
       when map_size(params) == 1,
       do: Review.workspace(actor, id, now)

  defp perform(actor, operation, %{"connection_id" => id, "input" => input} = params, now)
       when map_size(params) == 2 and is_map(input) do
    case operation do
      "inspect" -> Sources.inspect_source(actor, id, input, now)
      "share" -> Sources.share(actor, id, input, now)
      "prepare" -> Review.prepare_human(actor, id, input, now)
      "read" -> Review.read_human(actor, id, input, now)
      "edit" -> Review.edit(actor, id, input, now)
      "approve" -> Review.decide(actor, id, input, "approved", now)
      "reject" -> Review.decide(actor, id, input, "rejected", now)
      _ -> {:error, :invalid_request}
    end
  end

  defp perform(_, _, _, _), do: {:error, :invalid_request}

  defp enabled?,
    do:
      Enum.all?(
        [:customer_replay_enabled, :customer_evidence_enabled, :customer_connections_enabled],
        &Application.get_env(:query_service_ex, &1, false)
      )

  defp failure(conn, code)
       when code in [
              :review_conflict,
              :source_changed,
              :projection_not_enabled,
              :query_operation_conflict,
              :query_period_changed
            ],
       do: error(conn, 409, "review_conflict")

  defp failure(conn, code)
       when code in [:review_expired, :source_expired, :query_operation_expired],
       do: error(conn, 410, "review_expired")

  defp failure(conn, code)
       when code in [
              :invalid_request,
              :invalid_review,
              :invalid_source_request,
              :invalid_operation
            ],
       do: error(conn, 422, "invalid_request")

  defp failure(conn, code)
       when code in [:review_busy, :workspace_limit, :query_usage_busy, :query_operation_capacity],
       do: error(conn, 429, "review_busy")

  defp failure(conn, :query_quota_exceeded), do: error(conn, 402, "query_quota_exceeded")

  defp failure(conn, code) when code in [:not_found, :access_denied, :source_denied, :revoked],
    do: error(conn, 403, "access_denied")

  defp failure(conn, _), do: error(conn, 503, "access_unavailable")

  defp error(conn, status, code),
    do:
      conn
      |> put_resp_header("cache-control", "no-store")
      |> put_status(status)
      |> json(%{error: %{code: code}})
end
