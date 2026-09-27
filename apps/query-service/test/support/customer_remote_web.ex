defmodule QueryServiceEx.TestSupport.CustomerRemoteWeb do
  @moduledoc "Opt-in local Next.js proof through public HTTP routes, without following the Claude callback."
  import ExUnit.Assertions
  alias QueryServiceEx.TestSupport.CustomerRemoteHTTP, as: Remote

  def verify do
    url = System.fetch_env!("ALLSOURCE_REMOTE_WEB_URL")
    assert %URI{scheme: "http", host: "127.0.0.1", port: 4344} = URI.parse(url)

    client =
      Tesla.client(
        [{Tesla.Middleware.BaseUrl, url}, {Tesla.Middleware.Timeout, timeout: 15_000}],
        Tesla.Adapter.Hackney
      )

    assert %{status: 200} = get(client, "/.well-known/oauth-authorization-server")
    assert %{status: 200} = get(client, "/.well-known/oauth-protected-resource")
    assert %{status: 401} = post(client, "/mcp/customer-review", "{}", [])
    path = "/api/customer-agent/oauth/authorize?" <> URI.encode_query(Remote.request())
    assert %{status: 303} = prepared = get(client, path)
    assert header(prepared, "location") == url <> "/connect/claude"
    assert header(prepared, "referrer-policy") == "no-referrer"
    pending = cookie(prepared)
    assert header(prepared, "set-cookie") =~ "HttpOnly"
    assert header(prepared, "set-cookie") =~ "SameSite=lax"
    assert %{status: 200} = anonymous = get(client, "/connect/claude", pending)
    assert anonymous.body =~ "Sign in to continue"
    refute anonymous.body =~ Remote.request()["state"]

    assert header(anonymous, "content-security-policy") =~
             "form-action 'self' https://claude.ai/api/mcp/auth_callback"

    assert %{status: 200} =
             login =
             post(
               client,
               "/api/v1/auth/login",
               Jason.encode!(%{email: "connections@example.test", password: "synthetic-only"}),
               [{"origin", url}]
             )

    assert Jason.decode!(login.body)["session_established"]
    session = cookie(login)
    cookies = pending <> "; " <> session
    assert %{status: 200} = consent = get(client, "/connect/claude", cookies)
    assert consent.body =~ "Allow connection"
    assert consent.body =~ "connection-management-test"
    refute consent.body =~ Remote.request()["state"]

    for cookie <- [pending, session] do
      [_, value] = String.split(cookie, "=", parts: 2)
      refute consent.body =~ value
    end

    headers = [
      {"origin", url},
      {"content-type", "application/x-www-form-urlencoded"},
      {"cookie", cookies}
    ]

    assert %{status: 403} =
             post(
               client,
               "/api/customer-agent/oauth/decision",
               "decision=allow&consent=yes",
               List.keyreplace(headers, "origin", 0, {"origin", "https://evil.test"})
             )

    assert %{status: 303} =
             denied =
             post(client, "/api/customer-agent/oauth/decision", "decision=allow", headers)

    assert header(denied, "location") == url <> "/connect/claude?error=denied"

    assert %{status: 303} =
             allowed =
             post(
               client,
               "/api/customer-agent/oauth/decision",
               "decision=allow&consent=yes",
               headers
             )

    destination = URI.parse(header(allowed, "location"))

    assert destination.scheme == "https" and destination.host == "claude.ai" and
             destination.path == "/api/mcp/auth_callback"

    params = URI.decode_query(destination.query)
    assert params["state"] == Remote.request()["state"]
    assert params["iss"] == "https://www.example.test"
    # The redirect is inspected only; no credential or test data is sent to Anthropic.
    assert %{status: 200} =
             redeemed =
             post(
               client,
               "/api/customer-agent/oauth/token",
               URI.encode_query(Remote.exchange(params["code"])),
               [{"content-type", "application/x-www-form-urlencoded"}]
             )

    token = Jason.decode!(redeemed.body)["access_token"]
    assert is_binary(token)

    assert %{status: 200} =
             tools =
             post(
               client,
               "/mcp/customer-review",
               Jason.encode!(%{jsonrpc: "2.0", id: 1, method: "tools/list"}),
               Remote.bearer(token)
             )

    assert length(Jason.decode!(tools.body)["result"]["tools"]) == 2

    assert %{status: 400} =
             post(
               client,
               "/api/customer-agent/oauth/token",
               URI.encode_query(Remote.exchange(params["code"])),
               [{"content-type", "application/x-www-form-urlencoded"}]
             )

    assert %{status: 401} =
             post(
               client,
               "/mcp/customer-review",
               Jason.encode!(%{jsonrpc: "2.0", id: 2, method: "tools/list"}),
               Remote.bearer(token)
             )

    Remote.clear_rates()
  end

  defp get(client, path, cookie \\ "") do
    assert {:ok, response} = Tesla.get(client, path, headers: [{"cookie", cookie}])
    response
  end

  defp post(client, path, body, headers) do
    headers =
      if List.keymember?(headers, "content-type", 0),
        do: headers,
        else: [{"content-type", "application/json"} | headers]

    assert {:ok, response} = Tesla.post(client, path, body, headers: headers)
    response
  end

  defp header(response, name), do: response.headers |> List.keyfind(name, 0) |> elem(1)

  defp cookie(response),
    do: response |> header("set-cookie") |> String.split(";", parts: 2) |> hd()
end
