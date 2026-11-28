defmodule QueryServiceEx.Application.Services.EventHistoryAccess do
  @moduledoc "Current workspace authority and durable consent for Event History."
  alias QueryServiceEx.Domain.CustomerAgent.{ConnectionGrant, Eligibility, ReviewOwner}
  alias QueryServiceEx.Infrastructure.Adapters.EventHistoryStore

  def scope, do: "allsource.history.read"
  def store, do: Application.get_env(:query_service_ex, :event_history_store, EventHistoryStore)
  defp access, do: Application.fetch_env!(:query_service_ex, :customer_agent_access_store)

  def config do
    origin = Application.get_env(:query_service_ex, :event_history_origin)
    registrations = Application.get_env(:query_service_ex, :event_history_clients, %{})

    with true <- Application.get_env(:query_service_ex, :event_history_enabled, false),
         true <- ConnectionGrant.valid_resource?(origin) and URI.parse(origin).path in [nil, ""],
         true <- is_map(registrations) and map_size(registrations) in 1..16,
         true <- Enum.all?(registrations, &valid_registration?/1) do
      {:ok,
       %{
         origin: origin,
         issuer: origin <> "/event-history",
         resource: origin <> "/mcp/event-history",
         clients: registrations
       }}
    else
      _ -> {:error, :disabled}
    end
  end

  defp valid_registration?(
         {id,
          %{"host" => host, "redirect_uri" => redirect, "token_endpoint_auth_method" => method} =
            reg}
       ) do
    ConnectionGrant.valid_id?(id) and host in ["ChatGPT", "Claude"] and
      ConnectionGrant.valid_resource?(redirect) and
      ((method == "none" and is_nil(reg["client_secret"])) or
         (method in ["client_secret_post", "client_secret_basic"] and
            is_binary(reg["client_secret"]) and byte_size(reg["client_secret"]) >= 32))
  end

  defp valid_registration?(_), do: false

  def workspaces(actor, config) do
    with {:ok, ids} <- store().candidates(actor["subject_id"]) do
      Enum.reduce_while(ids, {:ok, []}, fn id, {:ok, acc} ->
        case workspace(actor["subject_id"], id, config) do
          {:ok, w} -> {:cont, {:ok, [w | acc]}}
          {:error, :access_denied} -> {:cont, {:ok, acc}}
          error -> {:halt, error}
        end
      end)
      |> case do
        {:ok, values} -> {:ok, %{workspaces: Enum.sort_by(values, &{&1["name"], &1["id"]})}}
        error -> error
      end
    end
  end

  def workspace(subject, tenant, config) do
    binding = %{
      "subject_id" => subject,
      "tenant_id" => tenant,
      "client_id" => "event-history",
      "resource" => config.resource
    }

    with true <- ConnectionGrant.valid_binding?(binding),
         {:ok, members} <- access().members(tenant),
         {:ok, record} <- access().tenant(tenant),
         {:ok, _} <-
           Eligibility.check_metered(record, members, binding, System.system_time(:second)),
         name when is_binary(name) and byte_size(name) in 1..256 <- record["name"] do
      {:ok, %{"id" => tenant, "name" => name}}
    else
      {:error, :storage_unavailable} = e -> e
      _ -> {:error, :access_denied}
    end
  end

  def principal(token, config) do
    with {:ok,
          %{
            "subject" => subject,
            "grant" => id,
            "resource" => resource,
            "client_id" => client,
            "workspace_id" => tenant
          }} <-
           open("access", token, 3600),
         true <- resource == config.resource,
         {:ok, registry, _} <- store().grants(subject),
         %{
           "status" => "active",
           "consent_version" => "event-history-v1",
           "scope" => scope,
           "client_id" => ^client,
           "workspace_id" => ^tenant,
           "resource" => ^resource
         } = grant <-
           registry[id],
         true <-
           scope == scope() and grant["expires_at"] > now() and
             is_map(config.clients[grant["client_id"]]) and
             config.clients[grant["client_id"]]["host"] == grant["host"],
         {:ok, workspace} <- workspace(subject, grant["workspace_id"], config) do
      {:ok, %{subject: subject, id: id, grant: grant, workspace: workspace}}
    else
      {:error, :storage_unavailable} = e -> e
      _ -> {:error, :invalid_token}
    end
  end

  def connections(actor, config) do
    subject = actor["subject_id"]

    with {:ok, reg, _} <- store().grants(subject),
         {:ok, %{workspaces: spaces}} <- workspaces(actor, config) do
      rows =
        for {id, g} <- reg,
            g["status"] == "active" and g["expires_at"] > now(),
            space <- spaces,
            space["id"] == g["workspace_id"],
            do: %{
              id: id,
              host: g["host"],
              workspace_id: space["id"],
              workspace_name: space["name"]
            }

      {:ok, %{connections: rows}}
    end
  end

  def revoke(subject, id, tenant) do
    update(subject, fn reg ->
      case reg[id] do
        %{"workspace_id" => ^tenant} = grant ->
          revoked = %{grant | "status" => "revoked", "sessions" => %{}}
          {:ok, Map.put(reg, id, revoked), %{revoked: true}}

        _ ->
          {:error, :access_denied}
      end
    end)
  end

  def update(subject, fun, attempts \\ 4)
  def update(_, _, 0), do: {:error, :storage_unavailable}

  def update(subject, fun, attempts) do
    with {:ok, old, rev} <- store().grants(subject),
         live =
           Map.filter(old, fn {_, g} ->
             is_integer(g["expires_at"]) and g["expires_at"] > now()
           end),
         {:ok, next, result} <- fun.(live),
         true <- map_size(next) <= 32 do
      case store().replace(subject, next, rev) do
        :ok -> {:ok, result}
        {:error, :conflict} -> update(subject, fun, attempts - 1)
        error -> error
      end
    else
      false -> {:error, :capacity}
      error -> error
    end
  end

  def seal(purpose, payload, age) do
    case System.get_env("JWT_SECRET") do
      key when is_binary(key) and byte_size(key) >= 32 ->
        {:ok,
         Plug.Crypto.encrypt(key, "allsource-event-history-v1/" <> purpose, payload, max_age: age)}

      _ ->
        {:error, :storage_unavailable}
    end
  end

  def open(purpose, token, age) when is_binary(token) and byte_size(token) in 1..8192 do
    with key when is_binary(key) and byte_size(key) >= 32 <- System.get_env("JWT_SECRET"),
         {:ok, value} <-
           Plug.Crypto.decrypt(key, "allsource-event-history-v1/" <> purpose, token, max_age: age),
         do: {:ok, value},
         else: (_ -> {:error, :invalid_request})
  end

  def open(_, _, _), do: {:error, :invalid_request}
  def now, do: System.system_time(:second)
  def nonce, do: Base.encode16(:crypto.strong_rand_bytes(16), case: :lower)
  def digest(value), do: ReviewOwner.digest(value)
end
