defmodule QueryServiceExWeb.Endpoint do
  @moduledoc """
  Phoenix endpoint for the Query Service API.

  Provides RESTful HTTP endpoints for querying events, managing projections,
  and executing queries through the DSL.

  Also exposes WebSocket endpoints for real-time event streaming via Phoenix Channels.
  """

  use Phoenix.Endpoint, otp_app: :query_service_ex

  # WebSocket endpoint for real-time event streaming
  socket("/ws", QueryServiceExWeb.UserSocket,
    websocket: true,
    longpoll: false
  )

  # Serve at "/" the static files from "priv/static" directory.
  #
  # You should set gzip to true if you are running phx.digest
  # when deploying your static files in production.
  plug(Plug.Static,
    at: "/",
    from: :query_service_ex,
    gzip: false,
    only: QueryServiceExWeb.static_paths()
  )

  # Code reloading can be explicitly enabled under the
  # :code_reloader configuration of your endpoint.
  if code_reloading? do
    plug(Phoenix.CodeReloader)
  end

  plug(Plug.RequestId)
  plug(QueryServiceExWeb.Plugs.CorrelationId)
  plug(Plug.Telemetry, event_prefix: [:phoenix, :endpoint])
  plug(QueryServiceExWeb.Plugs.RequestLogger)
  plug(QueryServiceExWeb.Plugs.CustomerAgentBody)

  plug(Plug.Parsers,
    parsers: [:json],
    pass: ["*/*"],
    json_decoder: Jason,
    body_reader: {QueryServiceExWeb.Plugs.RawBodyReader, :read_body, []}
  )

  plug(Plug.MethodOverride)
  plug(Plug.Head)

  # #9 fix: credentials: false — cross-origin requests use Authorization
  # header / X-API-Key, not cookies. This also avoids the browser-level
  # conflict where Access-Control-Allow-Origin: * + credentials: true is
  # rejected by all browsers.
  plug(CORSPlug,
    origin: &QueryServiceExWeb.Endpoint.cors_origins/0,
    credentials: false,
    methods: ["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS"],
    headers: [
      "authorization",
      "content-type",
      "x-api-key",
      "x-correlation-id",
      "x-requested-with"
    ],
    expose: ["x-correlation-id"],
    max_age: 86_400
  )

  plug(QueryServiceExWeb.Router)

  @doc false
  def cors_origins do
    Application.get_env(:query_service_ex, :cors_origins) ||
      ["https://www.all-source.xyz", "https://all-source.xyz"]
  end
end
