defmodule QueryServiceExWeb.CustomerAgentController do
  @moduledoc """
  Restricted customer MCP context and syntax validation. Disabled until enabled
  with an exact configured resource. No source retrieval, issuance, preparation,
  human approval or execution is exposed here.
  """

  use Phoenix.Controller, formats: [:json]
  import Plug.Conn

  alias QueryServiceEx.Application.Services.CustomerAgentAccess
  alias QueryServiceEx.Application.Services.CustomerAgentReview
  alias QueryServiceEx.Domain.CustomerAgent.Proposal
  alias QueryServiceEx.RateLimiter

  def context(conn, params), do: dispatch(conn, params, "read_context")
  def validate(conn, params), do: dispatch(conn, params, "validate_proposal")

  defp dispatch(conn, params, operation) do
    with true <- Application.get_env(:query_service_ex, :customer_review_enabled, false),
         true <- conn.query_string == "",
         {:allow, _} <- RateLimiter.check_rate("customer-review:admission", :free),
         :ok <- shape(params, operation),
         {:ok, token} <- credential(conn),
         true <- audience?(params["binding"]),
         now = System.system_time(:second),
         {:ok, access} <- CustomerAgentAccess.verify(token, params["binding"], operation, now),
         {:allow, _} <- RateLimiter.check_rate("customer-review:" <> access["tenant_id"], :free),
         {:ok, result} <- result(operation, params, access),
         {:ok, _} <-
           CustomerAgentAccess.verify(
             token,
             params["binding"],
             operation,
             System.system_time(:second)
           ) do
      conn |> put_resp_header("cache-control", "no-store") |> json(%{data: result})
    else
      {:error, :storage_unavailable} -> error(conn, 503, "access_unavailable")
      {:error, :invalid_proposal} -> error(conn, 422, "invalid_proposal")
      {:error, :invalid_request} -> error(conn, 400, "invalid_request")
      {:deny, _} -> conn |> put_resp_header("retry-after", "1") |> error(429, "rate_limited")
      _ -> error(conn, 403, "access_denied")
    end
  rescue
    _ -> error(conn, 503, "access_unavailable")
  end

  defp shape(params, operation) when is_map(params) do
    expected = if operation == "read_context", do: ["binding"], else: ["binding", "proposal"]
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

  defp result("read_context", _params, access) do
    {:ok,
     %{
       state: "eligibility_verified",
       mcp_scope: access["mcp_scope"],
       operations:
         Enum.filter(["read_context", "validate_proposal"], &(&1 in access["granted_operations"])),
       source_access: "unresolved",
       human_approval: "required_in_product",
       preparation_available: false
     }}
  end

  defp result("validate_proposal", %{"proposal" => input}, _access) do
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
