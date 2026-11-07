use thiserror::Error;

/// Everything that can go wrong producing or shipping a `geo.*` event.
#[derive(Debug, Error)]
pub enum GeoError {
    /// No credential in the environment. This is deliberately fatal: silently
    /// no-op'ing would leave a GEO window with a hole in it that nobody
    /// notices until the trend line is already wrong.
    #[error(
        "Neither ALLSOURCE_CLIENT_SIGNING_KEY nor ALLSOURCE_API_KEY is set — GEO telemetry \
         has nowhere to go.\n\
         \n\
         GEO events are written through the Control Plane gateway (default \
         https://api.all-source.xyz), never to Core directly.\n\
         \n\
         Fix one of:\n\
           export ALLSOURCE_CLIENT_SIGNING_KEY=<pem or path>   # a registered client key, \
         with ALLSOURCE_CLIENT_ID, ALLSOURCE_TENANT_ID and ALLSOURCE_CLIENT_KID \
         (docs/runbooks/CLIENT_KEY_ROTATION.md)\n\
           export ALLSOURCE_API_KEY=<key>     # mint one in the dashboard, or \
         POST /api/v1/onboard/start\n\
           export ALLSOURCE_API_URL=<url>     # optional, defaults to \
         https://api.all-source.xyz\n\
         \n\
         Or re-run with --dry-run to print the events instead of emitting them."
    )]
    MissingApiKey,

    /// `ALLSOURCE_CLIENT_SIGNING_KEY` is set but the registered client key
    /// cannot sign: an id is missing, or the key is unreadable or not P-256.
    #[error("registered client key: {reason}")]
    ClientKey { reason: String },

    /// `ALLSOURCE_API_URL` was set but empty.
    #[error("ALLSOURCE_API_URL is set but empty — unset it to use the default, or give it a URL")]
    EmptyApiUrl,

    /// The payload would not serialise. Only reachable if a payload type grows
    /// a non-JSON-representable field.
    #[error("failed to serialise {event_type} payload: {source}")]
    Serialize {
        event_type: &'static str,
        source: serde_json::Error,
    },

    /// The gateway rejected, or could not be reached for, an ingest.
    #[error("gateway ingest of {event_type} (entity {entity_id}) failed: {source}")]
    Ingest {
        event_type: &'static str,
        entity_id: String,
        source: allsource::Error,
    },

    /// The gateway rejected, or could not be reached for, a query.
    #[error("gateway query failed: {0}")]
    Query(#[source] allsource::Error),

    /// The compiled-in probe set is not a valid set.
    ///
    /// Always an authoring mistake in `tooling/geo/prompts/*.toml`: the set is
    /// `include_str!`d, so this cannot be caused by anything on the machine
    /// running the probe.
    #[error("the {family} probe set is invalid: {reason}")]
    PromptSet {
        family: &'static str,
        reason: String,
    },

    /// The SDK client could not be constructed (bad base URL, TLS setup, ...).
    #[error("could not build an AllSource client for {api_url}: {source}")]
    Client {
        api_url: String,
        source: allsource::Error,
    },
}
