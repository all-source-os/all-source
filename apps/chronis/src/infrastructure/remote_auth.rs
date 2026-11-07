//! The credential chronis sends to a remote Core or the hosted gateway.
//!
//! A registered client key signs a fresh 60-second ES256 assertion through the
//! `allsource` SDK. `bearer()` is called for every HTTP request and every
//! WebSocket connect, so a reconnect never replays an expired assertion.

use std::{fmt, sync::Arc};

use allsource::{CredentialProvider, Es256AssertionProvider};

use crate::domain::error::ChronError;

/// Audience the hosted gateway expects unless it is configured otherwise.
pub const DEFAULT_ASSERTION_AUDIENCE: &str = "https://api.all-source.xyz";

pub const ENV_CLIENT_SIGNING_KEY: &str = "ALLSOURCE_CLIENT_SIGNING_KEY";
pub const ENV_CLIENT_ID: &str = "ALLSOURCE_CLIENT_ID";
pub const ENV_TENANT_ID: &str = "ALLSOURCE_TENANT_ID";
pub const ENV_CLIENT_KID: &str = "ALLSOURCE_CLIENT_KID";
pub const ENV_ASSERTION_AUDIENCE: &str = "ALLSOURCE_ASSERTION_AUDIENCE";

/// How requests to the remote authenticate.
#[derive(Clone, Default)]
pub enum RemoteAuth {
    /// No `Authorization` header (a local Core with auth disabled).
    #[default]
    None,
    /// A long-lived API key sent unchanged.
    ApiKey(String),
    /// A registered client key; each call may mint a new assertion.
    Assertion(Arc<dyn CredentialProvider>),
}

impl RemoteAuth {
    /// Wrap a provider built by the caller.
    pub fn assertion(provider: impl CredentialProvider + 'static) -> Self {
        Self::Assertion(Arc::new(provider))
    }

    /// The bearer token for one request or connect, without the `Bearer ` prefix.
    pub fn bearer(&self) -> Result<Option<String>, ChronError> {
        match self {
            Self::None => Ok(None),
            Self::ApiKey(key) => Ok(Some(key.clone())),
            Self::Assertion(provider) => provider
                .bearer()
                .map(Some)
                .map_err(|e| ChronError::Sync(format!("client assertion: {e}"))),
        }
    }
}

impl fmt::Debug for RemoteAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => f.write_str("RemoteAuth::None"),
            Self::ApiKey(_) => f.write_str("RemoteAuth::ApiKey(<redacted>)"),
            Self::Assertion(_) => f.write_str("RemoteAuth::Assertion"),
        }
    }
}

/// The registered client key settings, after env overrides are applied.
pub struct ClientKeySettings<'a> {
    /// A PKCS#8 PEM, or a path to one.
    pub signing_key: &'a str,
    pub client_id: Option<&'a str>,
    pub tenant_id: Option<&'a str>,
    pub kid: Option<&'a str>,
    pub audience: Option<&'a str>,
}

/// Build the assertion provider. Every field but the audience is required, and
/// an unreadable or non-P-256 key fails here instead of on the first request.
pub fn client_key_auth(settings: &ClientKeySettings<'_>) -> Result<RemoteAuth, ChronError> {
    let (Some(client_id), Some(tenant_id), Some(kid)) =
        (settings.client_id, settings.tenant_id, settings.kid)
    else {
        return Err(ChronError::Sync(
            "a client signing key needs client_id, tenant_id and client_kid \
             ([sync] in config.toml, or ALLSOURCE_CLIENT_ID / ALLSOURCE_TENANT_ID / \
             ALLSOURCE_CLIENT_KID)"
                .into(),
        ));
    };
    let pem = read_signing_key(settings.signing_key)?;
    let audience = settings.audience.unwrap_or(DEFAULT_ASSERTION_AUDIENCE);
    let built = Es256AssertionProvider::from_pkcs8_pem(&pem, client_id, tenant_id, kid, audience);
    let provider = built.map_err(|e| ChronError::Sync(e.to_string()))?;
    Ok(RemoteAuth::assertion(provider))
}

/// Inline PEM, or a path to one. A leading `~/` resolves against `$HOME`.
fn read_signing_key(source: &str) -> Result<Vec<u8>, ChronError> {
    if source.contains("-----BEGIN") {
        return Ok(source.as_bytes().to_vec());
    }
    let path = match source.strip_prefix("~/") {
        Some(rest) => match std::env::var_os("HOME") {
            Some(home) => std::path::PathBuf::from(home).join(rest),
            None => std::path::PathBuf::from(source),
        },
        None => std::path::PathBuf::from(source),
    };
    std::fs::read(&path).map_err(|e| {
        ChronError::Sync(format!(
            "client signing key {} is not readable: {e}",
            path.display()
        ))
    })
}
