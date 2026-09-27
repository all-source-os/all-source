defmodule QueryServiceExWeb.CustomerAgentController do
  @moduledoc """
  Restricted customer MCP operations with exact resource binding. Evidence
  preparation is separately gated; human approval and execution are never exposed.
  """

  use Phoenix.Controller, formats: [:json]
  import Plug.Conn

  alias QueryServiceEx.Application.Services.CustomerAgentAccess
  alias QueryServiceEx.Application.Services.CustomerAgentReview
  alias QueryServiceEx.Application.Services.CustomerEvidenceReview
  alias QueryServiceEx.Application.Services.CustomerRemoteAuthorization
  alias QueryServiceEx.Application.Services.CustomerReplayReview
  alias QueryServiceEx.Domain.CustomerAgent.ConnectionConsent
  alias QueryServiceEx.Domain.CustomerAgent.Proposal
  alias QueryServiceEx.RateLimiter

  def context(conn, params), do: dispatch(conn, params, "read_context")
  def validate(conn, params), do: dispatch(conn, params, "validate_proposal")
  def prepare(conn, params), do: dispatch(conn, params, "prepare_proposal")
  def review(conn, params), do: dispatch(conn, params, "read_review")
  def result(conn, params), do: dispatch(conn, params, "read_result")
  def remote_session(conn, params), do: remote(conn, params, "session")
  def remote_context(conn, params), do: remote(conn, params, "read_context")
  def remote_validate(conn, params), do: remote(conn, params, "validate_proposal")
  def remote_prepare(conn, params), do: remote(conn, params, "prepare_proposal")
  def remote_review(conn, params), do: remote(conn, params, "read_review")
  def remote_result(conn, params), do: remote(conn, params, "read_result")

  defp remote(conn, params, operation) do
    with true <- Application.get_env(:query_service_ex, :customer_remote_enabled, false),
         {:allow, _} <- RateLimiter.check_rate("customer-remote:admission", :free),
         true <- is_map(params) and not Map.has_key?(params, "binding"),
         ["Bearer " <> envelope] <- get_req_header(conn, "authorization"),
         {:ok, payload} <-
           CustomerRemoteAuthorization.open_access(
             envelope,
             Application.get_env(:query_service_ex, :customer_review_resource),
             System.system_time(:second)
           ) do
      conn
      |> put_req_header("authorization", "Bearer " <> payload["token"])
      |> dispatch(Map.put(params, "binding", payload["binding"]), operation)
    else
      {:deny, _} -> conn |> put_resp_header("retry-after", "1") |> error(429, "rate_limited")
      _ -> error(conn, 401, "access_denied")
    end
  end

  defp dispatch(conn, params, operation) do
    with true <- Application.get_env(:query_service_ex, :customer_review_enabled, false),
         true <- enabled?(operation),
         true <- conn.query_string == "",
         {:allow, _} <- RateLimiter.check_rate("customer-review:admission", :free),
         :ok <- shape(params, operation),
         {:ok, token} <- credential(conn),
         true <- audience?(params["binding"]),
         now = System.system_time(:second),
         {:ok, access} <- verify(token, params["binding"], operation, now),
         {:allow, _} <- RateLimiter.check_rate("customer-review:" <> access["tenant_id"], :free),
         {:ok, result} <- perform(operation, params, access, token, now),
         {:ok, _} <-
           verify(
             token,
             params["binding"],
             operation,
             System.system_time(:second)
           ) do
      conn |> put_resp_header("cache-control", "no-store") |> json(%{data: result})
    else
      {:error, :storage_unavailable} ->
        error(conn, 503, "access_unavailable")

      {:error, :invalid_proposal} ->
        error(conn, 422, "invalid_proposal")

      {:error, :invalid_request} ->
        error(conn, 400, "invalid_request")

      {:error, code}
      when code in [:invalid_preparation, :unsupported_evidence, :invalid_operation] ->
        error(conn, 422, "invalid_proposal")

      {:error, :query_quota_exceeded} ->
        error(conn, 402, "query_quota_exceeded")

      {:error, code}
      when code in [
             :idempotency_conflict,
             :query_operation_conflict,
             :query_period_changed,
             :source_changed,
             :review_conflict,
             :projection_not_enabled
           ] ->
        error(conn, 409, "review_conflict")

      {:error, code} when code in [:source_expired, :query_operation_expired] ->
        error(conn, 410, "review_expired")

      {:error, code} when code in [:review_busy, :query_usage_busy, :query_operation_capacity] ->
        error(conn, 429, "review_busy")

      {:error, code}
      when code in [:review_unavailable, :query_usage_unavailable, :clock_moved_backwards] ->
        error(conn, 503, "access_unavailable")

      {:deny, _} ->
        conn |> put_resp_header("retry-after", "1") |> error(429, "rate_limited")

      _ ->
        error(conn, 403, "access_denied")
    end
  rescue
    _ -> error(conn, 503, "access_unavailable")
  end

  defp shape(params, operation) when is_map(params) do
    expected =
      case operation do
        op when op in ["read_context", "session"] ->
          ~w(binding)

        "validate_proposal" ->
          ~w(binding proposal)

        "prepare_proposal" ->
          ~w(binding expected_revision idempotency_key proposal)

        _ ->
          if(Map.has_key?(params, "digest"),
            do: ~w(binding digest id request_id version),
            else: ~w(binding id request_id version)
          )
      end

    if Enum.sort(Map.keys(params)) == expected, do: :ok, else: {:error, :invalid_request}
  end

  defp shape(_, _), do: {:error, :invalid_request}

  defp credential(conn) do
    case get_req_header(conn, "authorization") do
      ["Bearer " <> token] when byte_size(token) <= 128 -> {:ok, token}
      _ -> {:error, :access_denied}
    end
  end

  defp audience?(%{"resource" => resource}) when is_binary(resource) do
    resource != "" and
      resource == Application.get_env(:query_service_ex, :customer_review_resource)
  end

  defp audience?(_), do: false

  defp enabled?(operation) when operation in ~w(prepare_proposal read_review read_result),
    do: Application.get_env(:query_service_ex, :customer_evidence_enabled, false)

  defp enabled?(_), do: true

  defp verify(token, binding, "session", now),
    do: CustomerAgentAccess.verify_metered(token, binding, "read_context", now)

  defp verify(token, binding, operation, now)
       when operation in ~w(prepare_proposal read_review read_result),
       do: CustomerAgentAccess.verify_metered(token, binding, operation, now)

  defp verify(token, binding, operation, now),
    do: CustomerAgentAccess.verify(token, binding, operation, now)

  defp perform("session", _, _, _, _), do: {:ok, %{state: "connection_verified"}}

  defp perform(
         "prepare_proposal",
         %{"proposal" => %{"kind" => "replay_plan"}} = params,
         _,
         token,
         now
       ) do
    if Application.get_env(:query_service_ex, :customer_replay_enabled, false),
      do:
        CustomerReplayReview.prepare(token, params["binding"], Map.delete(params, "binding"), now),
      else: {:error, :access_denied}
  end

  defp perform("prepare_proposal", params, _, token, now),
    do:
      CustomerEvidenceReview.prepare(token, params["binding"], Map.delete(params, "binding"), now)

  defp perform(operation, %{"digest" => _} = params, _, token, now)
       when operation in ~w(read_review read_result) do
    if Application.get_env(:query_service_ex, :customer_replay_enabled, false),
      do:
        CustomerReplayReview.read(
          token,
          params["binding"],
          Map.delete(params, "binding"),
          now,
          operation
        ),
      else: {:error, :access_denied}
  end

  defp perform(operation, params, _, token, now) when operation in ~w(read_review read_result),
    do:
      CustomerEvidenceReview.read(
        token,
        params["binding"],
        params["id"],
        params["version"],
        params["request_id"],
        now,
        operation
      )

  defp perform("read_context", _params, access, _, _) do
    evidence? =
      enabled?("prepare_proposal") and
        access["consent_version"] in ConnectionConsent.evidence_versions()

    operations =
      if evidence?,
        do: ConnectionConsent.evidence_operations(),
        else: ~w(read_context validate_proposal)

    {:ok,
     %{
       state: "eligibility_verified",
       mcp_scope: access["mcp_scope"],
       operations: Enum.filter(operations, &(&1 in access["granted_operations"])),
       source_access: "unresolved",
       human_approval: "required_in_product",
       preparation_available: evidence? and "prepare_proposal" in access["granted_operations"]
     }}
  end

  defp perform("validate_proposal", %{"proposal" => input}, _access, _, _) do
    case CustomerAgentReview.validate(input) do
      {:ok, proposal} ->
        {:ok,
         %{
           state: "valid_unresolved",
           fingerprint: Proposal.fingerprint(proposal),
           unknowns: Proposal.unknowns(proposal),
           persisted: false,
           approved: false
         }}

      _ ->
        {:error, :invalid_proposal}
    end
  end

  defp error(conn, status, code) do
    conn
    |> put_resp_header("cache-control", "no-store")
    |> put_status(status)
    |> json(%{error: %{code: code}})
  end
end
