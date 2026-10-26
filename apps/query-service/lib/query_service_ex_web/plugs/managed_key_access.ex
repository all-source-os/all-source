defmodule QueryServiceExWeb.Plugs.ManagedKeyAccess do
  @moduledoc "Operation scopes for managed JWTs; human and service credentials retain their own roles."

  import Plug.Conn
  alias QueryServiceEx.Infrastructure.Adapters.ApiKeyAuthority

  def init(opts), do: opts

  def call(conn, _opts) do
    if reserved?(conn.params),
      do: forbid(conn, "reserved credential lifecycle stream"),
      else: conn
  end

  def claims(%{method: method} = conn, %{"view_as" => true})
      when method in ["POST", "PUT", "PATCH", "DELETE"],
      do: {:ok, forbid(conn, "view-as sessions are read-only")}

  def claims(conn, claims) do
    case ApiKeyAuthority.kind(claims) do
      {:ok, :managed} ->
        case ApiKeyAuthority.authorize(claims) do
          {:ok, key} -> {:ok, key(conn, key)}
          _ -> {:error, "invalid, expired or revoked API key"}
        end

      {:ok, _} ->
        {:ok, conn}

      _ ->
        {:error, "invalid credential kind"}
    end
  end

  def key(conn, key) do
    required = required_scope(conn.method, canonical_path(conn))
    scopes = key["scopes"]
    conn = assign(conn, :api_key, key)

    if ApiKeyAuthority.valid_scopes?(scopes) and required in scopes and
         role_permits?(key["role"], required),
       do: guard_ingest(conn),
       else: forbid(conn, "insufficient API key scope")
  end

  defp role_permits?(role, scope)
       when scope in ~w(events:write projections:write schemas:write),
       do: role in ~w(admin developer serviceaccount)

  defp role_permits?(role, _), do: role in ~w(admin developer readonly serviceaccount)

  def guard_ingest(%{halted: true} = conn), do: conn

  def guard_ingest(conn) do
    if conn.method == "POST" and canonical_path(conn) in ["/api/events", "/api/events/batch"] and
         reserved?(conn.body_params),
       do: forbid(conn, "reserved credential lifecycle stream"),
       else: conn
  end

  defp canonical_path(conn) do
    path = "/" <> Enum.join(conn.path_info, "/")

    case path do
      "/api/v1/" <> rest -> "/api/" <> rest
      _ -> path
    end
  end

  defp required_scope("GET", path) do
    cond do
      path in ["/api/events", "/api/streams", "/api/event-types"] or
          String.starts_with?(path, "/api/events/") ->
        "events:read"

      path in ["/api/projections", "/api/projection-templates"] or
          String.starts_with?(path, "/api/projections/") ->
        "projections:read"

      String.starts_with?(path, "/api/analytics/") ->
        "queries:execute"

      path == "/api/schemas" or String.starts_with?(path, "/api/schemas/") ->
        "schemas:read"

      true ->
        nil
    end
  end

  defp required_scope("POST", path) when path in ["/api/events", "/api/events/batch"],
    do: "events:write"

  defp required_scope("POST", path) when path in ["/api/query", "/api/query/projected"],
    do: "queries:execute"

  defp required_scope("POST", "/api/projections"), do: "projections:write"
  defp required_scope("DELETE", "/api/projections/" <> _), do: "projections:write"
  defp required_scope("POST", "/api/schemas"), do: "schemas:write"

  defp required_scope(_, _), do: nil

  defp reserved?(value) when is_map(value) do
    entity = Map.get(value, "entity_id", "")
    event = Map.get(value, "event_type", "")

    (is_binary(entity) and String.starts_with?(entity, "apikey:")) or
      (is_binary(event) and String.starts_with?(event, "apikey.")) or
      reserved?(Map.get(value, "events", []))
  end

  defp reserved?(values) when is_list(values), do: Enum.any?(values, &reserved?/1)
  defp reserved?(_), do: false

  defp forbid(conn, message),
    do:
      conn
      |> put_status(:forbidden)
      |> Phoenix.Controller.json(%{error: %{code: "forbidden", message: message}})
      |> halt()
end
