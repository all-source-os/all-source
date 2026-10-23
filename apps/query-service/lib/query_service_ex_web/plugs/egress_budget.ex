defmodule QueryServiceExWeb.Plugs.EgressBudget do
  @moduledoc "Returns bounded responses only after durable byte settlement."
  import Plug.Conn
  alias QueryServiceEx.Edition
  alias QueryServiceEx.EgressBudget

  def init(options), do: options

  def call(conn, _options) do
    if Edition.community?() or management_path?(conn.request_path) do
      conn
    else
      case EgressBudget.reserve(conn.assigns[:tenant_id]) do
        {:ok, reservation} ->
          register_before_send(conn, fn response -> settle(response, reservation) end)

        {:error, status} ->
          conn |> send_resp(status, error_body(status)) |> halt()
      end
    end
  end

  defp settle(conn, reservation) do
    if conn.state != :set do
      EgressBudget.complete(reservation, 0)

      raise ArgumentError,
            "funded responses must be bounded; streaming, files and upgrades are disabled"
    end

    bytes = IO.iodata_length(conn.resp_body || "")
    bounded = bytes <= reservation.cap

    case EgressBudget.complete(reservation, if(bounded, do: bytes, else: 0)) do
      {:ok, _} when bounded -> conn
      {:ok, _} -> refuse(conn, 413)
      {:error, status} -> refuse(conn, status)
    end
  end

  defp refuse(conn, status) do
    conn
    |> delete_resp_header("content-length")
    |> put_resp_content_type("application/json")
    |> resp(status, error_body(status))
  end

  defp management_path?(path) do
    String.starts_with?(path, ["/api/billing", "/api/tenant/billing", "/api/v1/billing"])
  end

  defp error_body(status) do
    Jason.encode!(%{
      error: "egress_budget_unavailable",
      status: status,
      message:
        "Open billing to inspect usage and prepaid credit. Request a smaller page if the response exceeds its byte allowance."
    })
  end
end
