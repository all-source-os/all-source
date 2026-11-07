use std::sync::Arc;

use crate::error::AllsourceAuthError;

/// Produces the bearer token for one request.
///
/// Mirrors `allsource::CredentialProvider` without depending on the SDK; wrap
/// an SDK provider as `move || provider.bearer().map_err(|e| e.to_string())`.
/// Any `Fn() -> Result<String, String> + Send + Sync` closure is a provider.
pub trait CredentialProvider: Send + Sync {
    /// The bearer token, without the `Bearer ` prefix.
    fn bearer(&self) -> Result<String, String>;
}

impl<F> CredentialProvider for F
where
    F: Fn() -> Result<String, String> + Send + Sync,
{
    fn bearer(&self) -> Result<String, String> {
        self()
    }
}

#[derive(Clone)]
pub(crate) enum Credential {
    Static(String),
    Provider(Arc<dyn CredentialProvider>),
}

impl Credential {
    pub(crate) fn header(&self) -> Result<String, AllsourceAuthError> {
        let token = match self {
            Credential::Static(key) => key.clone(),
            // Status 0 marks a refusal raised before any request was sent.
            Credential::Provider(provider) => {
                provider
                    .bearer()
                    .map_err(|message| AllsourceAuthError::Api {
                        status: 0,
                        message: format!("credential provider failed: {message}"),
                    })?
            }
        };
        Ok(format!("Bearer {token}"))
    }
}
