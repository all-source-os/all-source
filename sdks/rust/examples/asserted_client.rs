//! Query the hosted gateway with a registered client key instead of an API key.
//!
//! An AllSource operator registers the public half of a P-256 key on the
//! tenant and returns its kid (docs/runbooks/CLIENT_KEY_ROTATION.md). Every
//! request then carries a 60-second ES256 assertion signed with the private
//! key; the provider reuses one until 15 seconds before it expires.
//!
//! # Running
//!
//! ```bash
//! export ALLSOURCE_CLIENT_SIGNING_KEY=./client-key.pem   # PKCS#8 PEM path
//! export ALLSOURCE_CLIENT_ID=my-service                  # the assertion's iss
//! export ALLSOURCE_TENANT_ID=my-tenant                   # the assertion's sub
//! export ALLSOURCE_CLIENT_KID=<kid from registration>
//! # Optional: ALLSOURCE_API_URL and ALLSOURCE_ASSERTION_AUDIENCE,
//! # both default to https://api.all-source.xyz.
//! cargo run -p allsource --example asserted_client --features es256-assertion
//! ```

use allsource::{ClientConfig, Es256AssertionProvider, QueryClient, QueryEventsParams};

const GATEWAY: &str = "https://api.all-source.xyz";

fn required(name: &str) -> Result<String, Box<dyn std::error::Error>> {
    std::env::var(name).map_err(|_| format!("{name} is not set").into())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pem = std::fs::read(required("ALLSOURCE_CLIENT_SIGNING_KEY")?)?;
    let api_url = std::env::var("ALLSOURCE_API_URL").unwrap_or_else(|_| GATEWAY.to_string());
    let audience =
        std::env::var("ALLSOURCE_ASSERTION_AUDIENCE").unwrap_or_else(|_| GATEWAY.to_string());

    // Fails here, not on the first request, if the key cannot sign ES256.
    let provider = Es256AssertionProvider::from_pkcs8_pem(
        &pem,
        required("ALLSOURCE_CLIENT_ID")?,
        required("ALLSOURCE_TENANT_ID")?,
        required("ALLSOURCE_CLIENT_KID")?,
        audience,
    )?;

    // The API key is ignored when a credential provider is given.
    let config = ClientConfig::new(&api_url, "");
    let client = QueryClient::with_config_and_credential_provider(config, provider)?;

    let result = client
        .query_events(QueryEventsParams::new().limit(5))
        .await?;
    println!("{} events from {api_url}", result.count);
    for event in &result.events {
        println!(
            "{}  {}  {}",
            event.timestamp, event.event_type, event.entity_id
        );
    }
    Ok(())
}
