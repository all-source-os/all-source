unless Mix.env() == :test, do: raise("The isolated reviewer requires MIX_ENV=test")

[profile, file] = System.argv()
%File.Stat{type: :regular, mode: mode} = File.lstat!(file)
unless Bitwise.band(mode, 0o077) == 0, do: raise("Private fixture settings required")
settings = file |> File.read!() |> Jason.decode!()
origin = Map.fetch!(settings, "ALLSOURCE_EVENT_HISTORY_WEB_ORIGIN")
%URI{scheme: "https", host: host, userinfo: nil, query: nil, fragment: nil} = URI.new!(origin)
unless is_binary(host), do: raise("Public reviewer host required")

app =
  case profile do
    "mcp" ->
      "http://127.0.0.1:14902" = settings["ALLSOURCE_EVENT_HISTORY_QUERY_URL"]

      Application.put_all_env(
        mcp_server_elixir: [
          event_history: true,
          event_history_origin: origin,
          event_history_query_url: settings["ALLSOURCE_EVENT_HISTORY_QUERY_URL"],
          event_history_origins: [origin],
          customer_review: false,
          customer_review_http: false,
          customer_review_http_ip: "127.0.0.1",
          customer_review_http_port: 14904,
          start_stdio_server: false
        ]
      )

      :mcp_server_elixir

    "query" ->
      "http://127.0.0.1:14901" = core = Map.fetch!(settings, "CORE_URL")
      System.put_env("JWT_SECRET", Map.fetch!(settings, "JWT_SECRET"))
      System.put_env("AUTH_DISABLED", "false")
      clients = "ALLSOURCE_EVENT_HISTORY_CLIENTS" |> System.fetch_env!() |> Jason.decode!()

      Application.put_all_env(
        query_service_ex: [
          core_url: core,
          core_write_url: core,
          core_read_urls: [core],
          core_api_key: Map.fetch!(settings, "CORE_API_KEY"),
          core_ws_enabled: false,
          event_history_enabled: true,
          event_history_origin: origin,
          event_history_clients: clients
        ]
      )

      endpoint = Application.fetch_env!(:query_service_ex, QueryServiceExWeb.Endpoint)

      Application.put_env(
        :query_service_ex,
        QueryServiceExWeb.Endpoint,
        Keyword.merge(endpoint,
          server: true,
          http: [ip: {127, 0, 0, 1}, port: 14902],
          secret_key_base: Map.fetch!(settings, "SECRET_KEY_BASE")
        )
      )

      :query_service_ex
  end

Logger.configure(level: :warning)
{:ok, _} = Application.ensure_all_started(app)
Process.sleep(230_000)
