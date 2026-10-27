defmodule QueryServiceExWeb.ManagedKeyLifecycleTest do
  use ExUnit.Case, async: false
  import Plug.Conn
  import Plug.Test

  alias QueryServiceEx.ApiKeyStore
  alias QueryServiceEx.Infrastructure.Adapters.ApiKeyAuthority
  alias QueryServiceEx.Infrastructure.Adapters.RustCoreClient
  alias QueryServiceExWeb.ApiKeyController
  alias QueryServiceExWeb.Plugs.{ApiKeyAuth, AuthPipeline, JwksAuth, JwtAuth}

  @tenant "managed-lifecycle-test"
  @secret "synthetic-managed-lifecycle-secret-32-bytes"

  defmodule CoreFixture do
    import Plug.Conn
    def init(state), do: state

    def call(conn, state) do
      conn = fetch_query_params(conn)

      {event, conn} =
        if conn.method == "POST" do
          {:ok, body, updated} = read_body(conn, length: 65_536, read_timeout: 1_000)
          {Jason.decode!(body), updated}
        else
          {nil, conn}
        end

      {status, body} =
        case {conn.method, conn.request_path} do
          {"GET", "/api/v1/events/query"} ->
            Agent.get(state, fn s ->
              events = s.events

              {200,
               %{
                 "events" => events,
                 "count" => length(events),
                 "total_count" => length(events),
                 "has_more" => s.truncated,
                 "entity_version" =>
                   Map.get(
                     s,
                     :entity_version,
                     Enum.max(Enum.map(events, & &1["version"]), fn -> 0 end)
                   ),
                 "archive_integrity" => %{
                   "protocol" => "retained-entity-v1",
                   "tenant_id" => "managed-lifecycle-test",
                   "entity_id" => "apikey:managed-lifecycle-test"
                 }
               }}
            end)

          {"GET", "/api/v1/tenants/managed-lifecycle-test"} ->
            {200, %{"id" => "managed-lifecycle-test", "status" => "active"}}

          {"POST", "/api/v1/events"} ->
            Agent.get_and_update(state, &append_event(&1, event))

          _ ->
            {404, %{}}
        end

      conn |> put_resp_content_type("application/json") |> send_resp(status, Jason.encode!(body))
    end

    defp append_event(state, event) do
      if state.write_status == 201 do
        event =
          Map.merge(event, %{
            "id" => "event-#{length(state.events) + 1}",
            "version" => length(state.events) + 1,
            "timestamp" => DateTime.to_iso8601(DateTime.utc_now())
          })

        events =
          if String.starts_with?(event["event_type"], "apikey."),
            do: state.events ++ [event],
            else: state.events

        {{201, %{"event_id" => event["id"]}}, %{state | events: events}}
      else
        {{state.write_status, %{"error" => "synthetic persistence failure"}}, state}
      end
    end
  end

  setup do
    old_secret = System.get_env("JWT_SECRET")
    old_disabled = System.get_env("AUTH_DISABLED")
    old_jwks = System.get_env("AUTH_JWKS_URL")
    keys = [:core_url, :core_write_url, :core_read_urls]
    old_config = for key <- keys, do: {key, Application.get_env(:query_service_ex, key)}
    System.put_env("JWT_SECRET", @secret)
    System.delete_env("AUTH_DISABLED")
    System.delete_env("AUTH_JWKS_URL")

    state =
      start_supervised!({Agent, fn -> %{events: [], write_status: 201, truncated: false} end})

    server = start_supervised!({Bandit, plug: {CoreFixture, state}, port: 0, ip: {127, 0, 0, 1}})
    {:ok, {_, port}} = ThousandIsland.listener_info(server)
    url = "http://127.0.0.1:#{port}"
    Application.put_env(:query_service_ex, :core_url, url)
    Application.put_env(:query_service_ex, :core_write_url, url)
    Application.put_env(:query_service_ex, :core_read_urls, [url])
    :ets.delete(:api_key_store, {:keys, @tenant})

    on_exit(fn ->
      for {key, value} <- old_config do
        if is_nil(value),
          do: Application.delete_env(:query_service_ex, key),
          else: Application.put_env(:query_service_ex, key, value)
      end

      for {key, value} <- [
            {"JWT_SECRET", old_secret},
            {"AUTH_DISABLED", old_disabled},
            {"AUTH_JWKS_URL", old_jwks}
          ] do
        if value, do: System.put_env(key, value), else: System.delete_env(key)
      end

      :ets.delete(:api_key_store, {:keys, @tenant})
    end)

    %{state: state}
  end

  test "mint requested expiry, authenticate both headers, revoke, reject same JWT without cache" do
    expires = DateTime.add(DateTime.utc_now(), 600) |> DateTime.truncate(:second)

    response =
      create(%{"scopes" => ["events:read"], "expires_at" => DateTime.to_iso8601(expires)})

    assert response.status == 201
    key = Jason.decode!(response.resp_body)

    {:ok, claims} =
      RustCoreClient.decode_api_key_jwt(key["key"])

    assert claims["exp"] == DateTime.to_unix(expires)
    assert claims["credential_kind"] == "managed_api_key"

    for plug <- [AuthPipeline, JwksAuth, JwtAuth, ApiKeyAuth] do
      refute request(plug, key["key"]).halted
      denied = request(plug, key["key"], :post)
      assert denied.status == 403
    end

    assert :ok = ApiKeyStore.revoke_key(@tenant, key["id"])

    for plug <- [AuthPipeline, JwksAuth, JwtAuth, ApiKeyAuth] do
      denied = request(plug, key["key"])
      assert denied.halted
      assert denied.status == 401
    end
  end

  test "create and revoke failures never claim durable success", %{state: state} do
    Agent.update(state, &%{&1 | write_status: 503})
    response = create(%{"scopes" => ["events:read"]})
    assert response.status == 503
    refute Map.has_key?(Jason.decode!(response.resp_body), "key")
    assert ApiKeyStore.list_keys(@tenant) == []
    Agent.update(state, &%{&1 | write_status: 201})
    created = create(%{"scopes" => ["events:read"]}) |> Map.fetch!(:resp_body) |> Jason.decode!()
    Agent.update(state, &%{&1 | write_status: 503})
    assert {:error, _} = ApiKeyStore.revoke_key(@tenant, created["id"])
    refute request(AuthPipeline, created["key"]).halted
  end

  test "invalid options rejected before persistence", %{state: state} do
    for params <- [
          %{},
          %{"name" => 123, "scopes" => ["events:read"]},
          %{"scopes" => []},
          %{"scopes" => ["admin"]},
          %{"scopes" => ["events:read"], "expires_at" => "bad"},
          %{"scopes" => ["events:read"], "expires_at" => "2020-01-01T00:00:00Z"}
        ] do
      assert create(params).status == 400
    end

    assert Agent.get(state, & &1.events) == []
  end

  test "unknown key, incomplete authority, expiry, cross-tenant and reserved stream deny", %{
    state: state
  } do
    key =
      create(%{"scopes" => ["events:read", "events:write"]})
      |> Map.fetch!(:resp_body)
      |> Jason.decode!()

    Agent.update(state, &%{&1 | truncated: true})
    assert request(AuthPipeline, key["key"]).status == 401
    Agent.update(state, &%{&1 | truncated: false})

    conn =
      conn(:post, "/api/events", %{
        "entity_id" => "apikey:#{@tenant}",
        "event_type" => "apikey.created"
      })
      |> put_req_header("authorization", "Bearer " <> key["key"])
      |> AuthPipeline.call([])

    assert conn.status == 403

    Agent.update(state, fn s ->
      events =
        Enum.map(s.events, &put_in(&1, ["payload", "key", "expires_at"], "2020-01-01T00:00:00Z"))

      %{s | events: events}
    end)

    assert request(AuthPipeline, key["key"]).status == 401

    assert {:error, _} =
             ApiKeyAuthority.authorize(%{
               "is_api_key" => true,
               "sub" => "unknown",
               "tenant_id" => @tenant
             })
  end

  test "human and short service delegation retain access; long service bypass fails" do
    now = System.system_time(:second)

    base = %{
      "sub" => "synthetic-human",
      "tenant_id" => @tenant,
      "role" => "developer",
      "iat" => now,
      "exp" => now + 600,
      "iss" => "allsource"
    }

    for claims <- [
          base,
          Map.merge(base, %{"is_api_key" => true, "exp" => now + 60}),
          Map.merge(base, %{
            "is_api_key" => true,
            "credential_kind" => "service_delegation",
            "exp" => now + 60
          })
        ] do
      refute request(AuthPipeline, sign(claims)).halted
    end

    denied =
      request(
        AuthPipeline,
        sign(Map.merge(base, %{"is_api_key" => true, "credential_kind" => "service_delegation"}))
      )

    assert denied.status == 401
  end

  test "legacy version-one prefix retains admission and terminal revocation", %{state: state} do
    for versions <- [[1, 1, 3], [1, 1, 1], List.duplicate(1, 9) ++ [10]] do
      created =
        create(%{"scopes" => ["events:read"]}) |> Map.fetch!(:resp_body) |> Jason.decode!()

      original = Agent.get(state, &List.last(&1.events))
      older = put_in(original, ["payload", "key", "id"], "older-key")

      events =
        versions
        |> Enum.with_index(1)
        |> Enum.map(fn {version, ordinal} ->
          event = if ordinal == length(versions), do: original, else: older
          day = if version == 1, do: 1, else: ordinal

          Map.merge(event, %{
            "id" => "legacy-#{ordinal}",
            "version" => version,
            "timestamp" => "2026-01-#{String.pad_leading(to_string(day), 2, "0")}T00:00:00Z"
          })
        end)

      Agent.update(state, &%{&1 | events: events})

      for plug <- [AuthPipeline, JwksAuth, JwtAuth, ApiKeyAuth] do
        refute request(plug, created["key"]).halted
      end

      assert :ok = ApiKeyStore.revoke_key(@tenant, created["id"])

      for plug <- [AuthPipeline, JwksAuth, JwtAuth, ApiKeyAuth] do
        assert request(plug, created["key"]).status == 401
      end
    end
  end

  test "legacy prefix rejects incomplete, reordered and malformed authority", %{state: state} do
    created = create(%{"scopes" => ["events:read"]}) |> Map.fetch!(:resp_body) |> Jason.decode!()
    original = Agent.get(state, &hd(&1.events))

    events =
      Enum.map(1..3, fn ordinal ->
        Map.merge(original, %{
          "id" => "legacy-#{ordinal}",
          "version" => if(ordinal == 3, do: 3, else: 1),
          "timestamp" => "2026-01-0#{ordinal}T00:00:00Z"
        })
      end)

    [first, second, third] = events

    for malformed <- [
          [first, third],
          [first, second],
          [second, first, third],
          [first, %{second | "version" => 2}, %{third | "version" => 1}],
          [first, second, %{third | "version" => 4}],
          [first, second, %{third | "id" => first["id"]}],
          [first, second, %{third | "tenant_id" => "other-tenant"}],
          [first, second, %{third | "entity_id" => "apikey:other-tenant"}]
        ] do
      Agent.update(state, &Map.merge(&1, %{events: malformed, entity_version: 3}))

      for plug <- [AuthPipeline, JwksAuth, JwtAuth, ApiKeyAuth] do
        assert request(plug, created["key"]).status == 401
      end
    end

    Agent.update(state, &Map.merge(&1, %{events: [first, second], entity_version: 1.0}))

    for plug <- [AuthPipeline, JwksAuth, JwtAuth, ApiKeyAuth] do
      assert request(plug, created["key"]).status == 401
    end
  end

  defp create(params) do
    conn(:post, "/api/api-keys")
    |> assign(:tenant_id, @tenant)
    |> assign(:current_user, %{id: "synthetic-user", email: "user@example.test"})
    |> ApiKeyController.create(Map.put_new(params, "name", "synthetic"))
  end

  defp request(plug, token, method \\ :get, path \\ "/api/events") do
    header = if plug == ApiKeyAuth, do: "x-api-key", else: "authorization"
    value = if plug == ApiKeyAuth, do: token, else: "Bearer " <> token
    conn(method, path) |> put_req_header(header, value) |> plug.call([])
  end

  defp sign(claims) do
    {_, token} =
      JOSE.JWT.sign(JOSE.JWK.from_oct(@secret), %{"alg" => "HS256"}, claims) |> JOSE.JWS.compact()

    token
  end

  test "duplicate registration is idempotent and malformed sibling scopes stay isolated", %{
    state: state
  } do
    created = create(%{"scopes" => ["events:read"]}) |> Map.fetch!(:resp_body) |> Jason.decode!()

    Agent.update(state, fn s ->
      original = hd(s.events)
      duplicate = Map.merge(original, %{"id" => "duplicate", "version" => 2})

      other =
        original
        |> put_in(["payload", "key", "id"], "sibling-key")
        |> put_in(["payload", "key", "scopes"], [])
        |> Map.merge(%{"id" => "sibling", "version" => 3})

      %{s | events: [original, duplicate, other]}
    end)

    refute request(AuthPipeline, created["key"]).halted
    assert :ok = ApiKeyStore.revoke_key(@tenant, created["id"])

    Agent.update(state, fn s ->
      replay = hd(s.events) |> Map.merge(%{"id" => "replay", "version" => 5})
      %{s | events: s.events ++ [replay]}
    end)

    assert request(AuthPipeline, created["key"]).status == 401
  end

  test "delegation clock allowance stays bounded" do
    now = System.system_time(:second)

    claims = %{
      "is_api_key" => true,
      "credential_kind" => "service_delegation",
      "sub" => "synthetic-service",
      "tenant_id" => @tenant,
      "role" => "serviceaccount",
      "iss" => "allsource",
      "iat" => now + 3,
      "exp" => now + 63
    }

    refute request(AuthPipeline, sign(claims)).halted

    assert request(AuthPipeline, sign(%{claims | "iat" => now + 30, "exp" => now + 90})).status ==
             401
  end

  test "view-as session stays read-only through both JWT plugs" do
    now = System.system_time(:second)

    token =
      sign(%{
        "sub" => "synthetic-admin",
        "tenant_id" => @tenant,
        "view_as" => true,
        "role" => "readonly",
        "iat" => now,
        "exp" => now + 600,
        "iss" => "allsource"
      })

    for plug <- [AuthPipeline, JwksAuth, JwtAuth] do
      refute request(plug, token).halted
      assert request(plug, token, :post).status == 403
    end
  end

  test "JWKS fallback rejects malformed expiry and unknown non-expiring key" do
    created = create(%{"scopes" => ["events:read"]}) |> Map.fetch!(:resp_body) |> Jason.decode!()
    {:ok, claims} = RustCoreClient.decode_api_key_jwt(created["key"])
    refute Map.has_key?(claims, "exp")
    assert request(JwksAuth, sign(Map.put(claims, "exp", nil))).status == 401
    assert request(JwksAuth, sign(Map.put(claims, "sub", "unknown"))).status == 401
  end

  test "scopes never expand readonly role; markerless serviceaccounts are rejected" do
    created =
      create(%{"scopes" => ["events:read", "events:write"]})
      |> Map.fetch!(:resp_body)
      |> Jason.decode!()

    {:ok, claims} = RustCoreClient.decode_api_key_jwt(created["key"])
    readonly = sign(Map.put(claims, "role", "readonly"))

    for plug <- [AuthPipeline, JwksAuth, JwtAuth, ApiKeyAuth] do
      refute request(plug, readonly).halted
      assert request(plug, readonly, :post).status == 403
    end

    for plug <- [AuthPipeline, JwksAuth, JwtAuth] do
      assert request(plug, sign(Map.delete(claims, "is_api_key"))).status == 401
    end
  end

  test "non-expiring JWT cannot stand in for finite registration", %{state: state} do
    created = create(%{"scopes" => ["events:read"]}) |> Map.fetch!(:resp_body) |> Jason.decode!()
    refute request(AuthPipeline, created["key"]).halted
    future = DateTime.add(DateTime.utc_now(), 600) |> DateTime.to_iso8601()

    Agent.update(state, fn s ->
      %{s | events: Enum.map(s.events, &put_in(&1, ["payload", "key", "expires_at"], future))}
    end)

    assert request(AuthPipeline, created["key"]).status == 401
  end

  test "all dashboard scopes mint and authorize only matching operations within role bounds" do
    cases = [
      {:get, "/api/v1/events", "events:read", false},
      {:post, "/api/v1/events", "events:write", true},
      {:post, "/api/query", "queries:execute", false},
      {:get, "/api/projections", "projections:read", false},
      {:post, "/api/projections", "projections:write", true},
      {:delete, "/api/v1/projections/synthetic", "projections:write", true},
      {:get, "/api/schemas", "schemas:read", false},
      {:get, "/api/v1/schemas/synthetic", "schemas:read", false},
      {:post, "/api/v1/schemas", "schemas:write", true}
    ]

    for {method, path, scope, write?} <- cases do
      response = create(%{"scopes" => [scope]})
      assert response.status == 201
      key = Jason.decode!(response.resp_body)
      {:ok, claims} = RustCoreClient.decode_api_key_jwt(key["key"])
      assert claims["role"] == "serviceaccount"
      readonly = sign(Map.put(claims, "role", "readonly"))

      for plug <- [AuthPipeline, JwksAuth, JwtAuth, ApiKeyAuth] do
        refute request(plug, key["key"], method, path).halted
        assert request(plug, key["key"], :get, "/api/api-keys").status == 403

        if write?,
          do: assert(request(plug, readonly, method, path).status == 403),
          else: refute(request(plug, readonly, method, path).halted)
      end
    end
  end

  # The shape SignAPIKey minted for onboarded tenants: subject is the tenant id
  # and no apikey.created record registers it.
  defp legacy_onboarding_key do
    now = System.system_time(:second)

    sign(%{
      "sub" => @tenant,
      "tenant_id" => @tenant,
      "name" => "Longhand",
      "username" => "Longhand",
      "role" => "developer",
      "is_api_key" => true,
      "iss" => "allsource",
      "iat" => now,
      "exp" => now + 365 * 24 * 3600
    })
  end

  defp with_mode(mode) do
    Application.put_env(:query_service_ex, :key_authority_mode, mode)
    on_exit(fn -> Application.delete_env(:query_service_ex, :key_authority_mode) end)
  end

  defp count_denials(test_pid) do
    handler = "key-authority-denials-#{inspect(test_pid)}"

    :telemetry.attach(
      handler,
      [:query_service_ex, :auth, :key_authority_denial],
      fn _, _, meta, _ -> send(test_pid, {:denial, meta.mode, meta.reason}) end,
      nil
    )

    on_exit(fn -> :telemetry.detach(handler) end)
  end

  test "shadow serves an unregistered legacy onboarding key and counts it" do
    with_mode(:shadow)
    count_denials(self())
    key = legacy_onboarding_key()

    for plug <- [AuthPipeline, JwksAuth, JwtAuth, ApiKeyAuth] do
      refute request(plug, key).halted, "#{inspect(plug)} refused the legacy key in shadow"
      assert_received {:denial, :shadow, _reason}
    end

    reserved =
      conn(:post, "/api/events", %{
        "entity_id" => "apikey:#{@tenant}",
        "event_type" => "apikey.created"
      })
      |> put_req_header("authorization", "Bearer " <> key)
      |> AuthPipeline.call([])

    assert reserved.status == 403
  end

  test "enforce refuses the legacy onboarding key" do
    with_mode(:enforce)

    for plug <- [AuthPipeline, JwksAuth, JwtAuth, ApiKeyAuth] do
      assert request(plug, legacy_onboarding_key()).status == 401
    end
  end

  test "shadow still refuses a revoked key" do
    with_mode(:shadow)
    count_denials(self())
    created = create(%{"scopes" => ["events:read"]}) |> Map.fetch!(:resp_body) |> Jason.decode!()
    assert :ok = ApiKeyStore.revoke_key(@tenant, created["id"])

    for plug <- [AuthPipeline, JwksAuth, JwtAuth, ApiKeyAuth] do
      assert request(plug, created["key"]).status == 401
      assert_received {:denial, :shadow, :key_revoked}
    end
  end

  test "shadow serves a legacy key whose id a stray revocation names", %{state: state} do
    with_mode(:shadow)
    count_denials(self())

    stray = %{
      "id" => "stray",
      "tenant_id" => @tenant,
      "entity_id" => "apikey:#{@tenant}",
      "event_type" => "apikey.revoked",
      "version" => 1,
      "timestamp" => "2026-01-01T00:00:00Z",
      "payload" => %{"key_id" => @tenant}
    }

    Agent.update(state, &%{&1 | events: [stray]})

    for plug <- [AuthPipeline, JwksAuth, JwtAuth, ApiKeyAuth] do
      refute request(plug, legacy_onboarding_key()).halted
      assert_received {:denial, :shadow, :invalid_key}
    end
  end

  test "off skips the authority read", %{state: state} do
    with_mode(:off)
    Agent.update(state, &%{&1 | truncated: true})

    for plug <- [AuthPipeline, JwksAuth, JwtAuth, ApiKeyAuth] do
      refute request(plug, legacy_onboarding_key()).halted
    end
  end
end
