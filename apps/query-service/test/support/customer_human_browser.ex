defmodule QueryServiceEx.TestSupport.CustomerHumanBrowser do
  @moduledoc "Loopback-only synthetic dashboard session shell; evidence requests use the real Endpoint and Core."
  import Plug.Conn
  alias QueryServiceEx.TestSupport.MeteredEvidenceFixture, as: F
  alias QueryServiceExWeb.CustomerHumanSession

  def init(options), do: options

  def call(%{request_path: "/fixture"} = conn, _options) do
    conn
    |> put_resp_content_type("text/html")
    |> put_resp_header("cache-control", "no-store")
    |> send_resp(200, """
    <!doctype html><html lang="en"><meta charset="utf-8"><title>Synthetic evidence fixture</title>
    <body><h1>Local synthetic evidence fixture</h1><p>No customer or production data.</p>
    <p>Unshared run: 00000000-0000-4000-8000-000000000003</p>
    <form method="post" action="/fixture/session"><button>Enter synthetic workspace</button></form></body></html>
    """)
  end

  def call(%{request_path: "/fixture/session", method: "POST"} = conn, options) do
    conn
    |> put_resp_cookie("auth_token", options[:token],
      http_only: true,
      same_site: "Lax",
      max_age: 3600
    )
    |> put_resp_header("cache-control", "no-store")
    |> put_resp_header(
      "location",
      options[:target_url] || "http://127.0.0.1:4344/dashboard/tools/agent-reviews"
    )
    |> send_resp(303, "")
  end

  def call(%{request_path: path} = conn, _options) when path in ["/api/auth/me", "/api/tenant"] do
    case CustomerHumanSession.actor(conn) do
      {:ok, actor} ->
        if actor == F.actor(), do: session_data(conn, path), else: send_resp(conn, 403, "")

      _ ->
        send_resp(conn, 401, "")
    end
  end

  def call(conn, _),
    do: QueryServiceExWeb.Endpoint.call(conn, QueryServiceExWeb.Endpoint.init([]))

  defp session_data(conn, "/api/auth/me"),
    do:
      json(conn, %{
        data: %{
          user: %{
            id: F.actor()["subject_id"],
            email: "synthetic-evidence@example.test",
            name: "Synthetic evidence operator",
            provider: "google",
            role: "member"
          }
        }
      })

  defp session_data(conn, "/api/tenant"),
    do:
      json(conn, %{
        data: %{
          id: F.tenant(),
          name: "Synthetic evidence workspace",
          subscription_tier: "indie",
          is_demo: false
        }
      })

  defp json(conn, data),
    do:
      conn
      |> put_resp_content_type("application/json")
      |> put_resp_header("cache-control", "no-store")
      |> send_resp(200, Jason.encode!(data))
end
