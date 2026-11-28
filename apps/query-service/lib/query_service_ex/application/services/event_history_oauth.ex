defmodule QueryServiceEx.Application.Services.EventHistoryOAuth do
  @moduledoc "Workspace-selected S256 OAuth grants for registered Event History clients."
  alias QueryServiceEx.Application.Services.EventHistoryAccess, as: A

  def metadata(c),
    do:
      {:ok,
       %{
         issuer: c.issuer,
         authorization_endpoint: c.origin <> "/api/event-history/oauth/authorize",
         token_endpoint: c.origin <> "/api/event-history/oauth/token",
         response_types_supported: ["code"],
         grant_types_supported: ["authorization_code"],
         token_endpoint_auth_methods_supported:
           c.clients
           |> Map.values()
           |> Enum.map(& &1["token_endpoint_auth_method"])
           |> Enum.uniq(),
         code_challenge_methods_supported: ["S256"],
         scopes_supported: [A.scope()],
         authorization_response_iss_parameter_supported: true
       }}

  def resource(c),
    do:
      {:ok,
       %{
         resource: c.resource,
         authorization_servers: [c.issuer],
         scopes_supported: [A.scope()],
         bearer_methods_supported: ["header"]
       }}

  def prepare(c, %{"ui_locales" => locales} = params)
      when is_binary(locales) and byte_size(locales) in 1..256 do
    prepare(c, Map.delete(params, "ui_locales"))
  end

  def prepare(c, params) do
    with {:ok, _} <- registration(c, params),
         {:ok, handle} <- A.seal("request", %{request: params, id: A.nonce()}, 300) do
      {:ok, %{request_token: handle}}
    end
  end

  def inspect_request(c, %{"request_token" => token} = params) when map_size(params) == 1 do
    with {:ok, %{request: request}} <- A.open("request", token, 300),
         {:ok, client} <- registration(c, request) do
      {:ok,
       request
       |> Map.take(~w(redirect_uri scope state))
       |> Map.merge(%{"issuer" => c.issuer, "host" => client["host"]})}
    end
  end

  def inspect_request(_, _), do: {:error, :invalid_request}

  def decision(c, actor, %{"request_token" => token, "decision" => decision} = params) do
    with {:ok, %{request: request, id: id}} <- A.open("request", token, 300),
         {:ok, client} <- registration(c, request) do
      return = %{
        "redirect_uri" => request["redirect_uri"],
        "issuer" => c.issuer,
        "state" => request["state"]
      }

      case {decision, Map.keys(params) -- ~w(request_token decision)} do
        {"cancel", keys} when keys in [[], ["workspace_id"]] ->
          {:ok, Map.put(return, "error", "access_denied")}

        {"allow", ["workspace_id"]} ->
          issue(c, actor["subject_id"], params["workspace_id"], request, client, id, return)

        _ ->
          {:error, :invalid_request}
      end
    end
  end

  def decision(_, _, _), do: {:error, :invalid_request}

  defp issue(c, subject, workspace, request, client, id, return) do
    with {:ok, _} <- A.workspace(subject, workspace, c),
         {:ok, code} <-
           A.seal(
             "code",
             %{subject: subject, id: id, request: request, workspace: workspace},
             300
           ),
         {:ok, _} <-
           A.update(subject, fn reg ->
             if Map.has_key?(reg, id),
               do: {:error, :invalid_grant},
               else:
                 {:ok,
                  Map.put(reg, id, %{
                    "status" => "pending",
                    "workspace_id" => workspace,
                    "client_id" => request["client_id"],
                    "host" => client["host"],
                    "resource" => c.resource,
                    "scope" => A.scope(),
                    "consented_at" => A.now(),
                    "consent_version" => "event-history-v1",
                    "expires_at" => A.now() + 300,
                    "sessions" => %{}
                  }), :ok}
           end),
         {:ok, _} <- A.workspace(subject, workspace, c) do
      {:ok, Map.put(return, "code", code)}
    end
  end

  def redeem(c, params, auth) do
    with {:ok, id, fields} <- authenticate(c, params, auth),
         true <-
           Enum.sort(Map.keys(fields)) == ~w(code code_verifier grant_type redirect_uri resource),
         true <- fields["grant_type"] == "authorization_code",
         {:ok, %{subject: subject, id: grant, request: request, workspace: workspace}} <-
           A.open("code", fields["code"], 300),
         {:ok, _} <- registration(c, request),
         true <-
           request["client_id"] == id and fields["resource"] == c.resource and
             fields["redirect_uri"] == request["redirect_uri"],
         verifier when is_binary(verifier) and byte_size(verifier) in 43..128 <-
           fields["code_verifier"],
         true <- Regex.match?(~r/\A[A-Za-z0-9._~-]+\z/, verifier),
         true <- Plug.Crypto.secure_compare(sha(verifier), request["code_challenge"]),
         {:ok, _} <- A.workspace(subject, workspace, c),
         {:ok, token} <-
           A.seal(
             "access",
             %{
               "subject" => subject,
               "grant" => grant,
               "resource" => c.resource,
               "client_id" => id,
               "workspace_id" => workspace
             },
             3600
           ),
         {:ok, _} <-
           A.update(subject, fn reg ->
             case reg[grant] do
               %{"status" => "pending", "workspace_id" => ^workspace, "client_id" => ^id} = g ->
                 {:ok,
                  Map.put(reg, grant, %{g | "status" => "active", "expires_at" => A.now() + 3600}),
                  :ok}

               _ ->
                 {:error, :invalid_grant}
             end
           end),
         {:ok, _} <- A.principal(token, c) do
      {:ok, %{access_token: token, token_type: "Bearer", scope: A.scope(), expires_in: 3600}}
    else
      {:error, :storage_unavailable} = error -> error
      _ -> {:error, :invalid_grant}
    end
  end

  defp registration(c, params) when is_map(params) do
    with true <-
           Enum.sort(Map.keys(params)) ==
             ~w(client_id code_challenge code_challenge_method redirect_uri resource response_type scope state),
         client when is_map(client) <- c.clients[params["client_id"]],
         true <-
           params["redirect_uri"] == client["redirect_uri"] and params["resource"] == c.resource and
             params["scope"] == A.scope(),
         true <- params["response_type"] == "code" and params["code_challenge_method"] == "S256",
         state when is_binary(state) and byte_size(state) in 1..512 <- params["state"],
         true <- Regex.match?(~r/\A[\x21-\x7e]+\z/, state),
         challenge when is_binary(challenge) and byte_size(challenge) == 43 <-
           params["code_challenge"],
         {:ok, bytes} when byte_size(bytes) == 32 <- Base.url_decode64(challenge, padding: false),
         true <- Base.url_encode64(bytes, padding: false) == challenge do
      {:ok, client}
    else
      _ -> {:error, :invalid_request}
    end
  end

  defp registration(_, _), do: {:error, :invalid_request}

  defp authenticate(c, params, []) do
    {id, rest} = Map.pop(params, "client_id")

    case c.clients[id] do
      %{"token_endpoint_auth_method" => "none"} ->
        {:ok, id, rest}

      %{"token_endpoint_auth_method" => "client_secret_post", "client_secret" => secret} ->
        {provided, fields} = Map.pop(rest, "client_secret")

        if is_binary(provided) and Plug.Crypto.secure_compare(secret, provided),
          do: {:ok, id, fields},
          else: :error

      _ ->
        :error
    end
  end

  defp authenticate(c, params, ["Basic " <> encoded]) do
    with {:ok, raw} <- Base.decode64(encoded),
         [username, password] <- String.split(raw, ":", parts: 2),
         id = URI.decode_www_form(username),
         true <- Map.get(params, "client_id", id) == id,
         %{"token_endpoint_auth_method" => "client_secret_basic", "client_secret" => secret} <-
           c.clients[id],
         true <- Plug.Crypto.secure_compare(secret, URI.decode_www_form(password)) do
      {:ok, id, Map.delete(params, "client_id")}
    else
      _ -> :error
    end
  end

  defp authenticate(_, _, _), do: :error
  defp sha(v), do: :sha256 |> :crypto.hash(v) |> Base.url_encode64(padding: false)
end
