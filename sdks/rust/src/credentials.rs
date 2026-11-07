//! Per-request credentials.
//!
//! A static API key is sent unchanged on every request. A
//! [`CredentialProvider`] is asked for a bearer on every attempt, retries
//! included, and on every WebSocket connect, so a short-lived token never
//! outlives the request it was minted for.

use std::{fmt, sync::Arc};

use crate::Error;

/// Produces the bearer token for one request attempt.
///
/// Any `Fn() -> Result<String, Error> + Send + Sync` closure is a provider.
/// Return [`Error::Config`] when no token can be produced; the client does not
/// retry that error.
pub trait CredentialProvider: Send + Sync {
    /// The bearer token, without the `Bearer ` prefix.
    fn bearer(&self) -> Result<String, Error>;
}

impl<F> CredentialProvider for F
where
    F: Fn() -> Result<String, Error> + Send + Sync,
{
    fn bearer(&self) -> Result<String, Error> {
        self()
    }
}

#[derive(Clone)]
pub(crate) enum Credential {
    Static(String),
    Provider(Arc<dyn CredentialProvider>),
}

impl Credential {
    pub(crate) fn bearer(&self) -> Result<String, Error> {
        match self {
            Credential::Static(key) => Ok(key.clone()),
            Credential::Provider(provider) => provider.bearer(),
        }
    }
}

impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Credential::Static(_) => f.write_str("Credential::Static(<redacted>)"),
            Credential::Provider(_) => f.write_str("Credential::Provider"),
        }
    }
}
