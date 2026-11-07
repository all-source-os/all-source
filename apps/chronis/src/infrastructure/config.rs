use std::path::Path;

use serde::{Deserialize, Serialize};

use super::remote_auth::{
    ClientKeySettings, ENV_ASSERTION_AUDIENCE, ENV_CLIENT_ID, ENV_CLIENT_KID,
    ENV_CLIENT_SIGNING_KEY, ENV_TENANT_ID, RemoteAuth, client_key_auth,
};
use crate::domain::error::ChronError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CoreMode {
    #[default]
    Embedded,
    Remote,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Remote Core URL used as sync target (embedded mode) or primary backend (remote mode).
    pub remote_url: Option<String>,
    /// Optional API key for authenticating with a remote Core instance.
    pub api_key: Option<String>,
    /// Registered client id; the assertion's `iss`.
    #[serde(default)]
    pub client_id: Option<String>,
    /// Tenant the client key is registered on; the assertion's `sub`.
    #[serde(default)]
    pub tenant_id: Option<String>,
    /// The registered key's kid (its RFC 7638 thumbprint).
    #[serde(default)]
    pub client_kid: Option<String>,
    /// Path to the P-256 PKCS#8 private key. This file is tracked in git, so it
    /// holds a path and never the PEM itself.
    #[serde(default)]
    pub signing_key_path: Option<String>,
    /// Audience the gateway expects; defaults to `https://api.all-source.xyz`.
    #[serde(default)]
    pub audience: Option<String>,
}

fn default_instance_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChronisConfig {
    #[serde(default)]
    pub mode: CoreMode,
    /// Unique identifier for this chronis instance (auto-generated on init).
    #[serde(default = "default_instance_id")]
    pub instance_id: String,
    #[serde(default)]
    pub sync: Option<SyncConfig>,
}

impl ChronisConfig {
    /// Load config from `.chronis/config.toml`, falling back to defaults.
    pub fn load(workspace_root: &Path) -> Result<Self, ChronError> {
        let config_path = workspace_root.join(".chronis/config.toml");
        if !config_path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&config_path)?;
        toml::from_str(&content).map_err(|e| ChronError::Sync(format!("invalid config.toml: {e}")))
    }

    /// The remote Core URL, if configured.
    pub fn remote_url(&self) -> Option<&str> {
        self.sync.as_ref().and_then(|s| s.remote_url.as_deref())
    }

    /// The remote Core API key, if configured.
    pub fn api_key(&self) -> Option<&str> {
        self.sync.as_ref().and_then(|s| s.api_key.as_deref())
    }

    /// The credential for the remote, reading `ALLSOURCE_CLIENT_*` overrides
    /// from the process environment.
    pub fn remote_auth(&self) -> Result<RemoteAuth, ChronError> {
        self.remote_auth_with_env(|name| std::env::var(name).ok())
    }

    /// The credential for the remote. A configured signing key wins over
    /// `api_key`; each `ALLSOURCE_CLIENT_*` variable `env` returns overrides
    /// the matching `[sync]` field, and blank values count as unset.
    pub fn remote_auth_with_env(
        &self,
        env: impl Fn(&str) -> Option<String>,
    ) -> Result<RemoteAuth, ChronError> {
        let sync = self.sync.as_ref();
        let nonblank = |value: &String| {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_string())
        };
        let pick = |var: &str, field: Option<&String>| {
            env(var)
                .as_ref()
                .and_then(nonblank)
                .or_else(|| field.and_then(nonblank))
        };

        let configured_path = sync.and_then(|s| s.signing_key_path.as_ref());
        if configured_path.is_some_and(|path| path.contains("-----BEGIN")) {
            return Err(ChronError::Sync(
                "[sync] signing_key_path holds a PEM; config.toml is tracked in git, \
                 so give it a path, or put the PEM in ALLSOURCE_CLIENT_SIGNING_KEY"
                    .into(),
            ));
        }

        if let Some(signing_key) = pick(ENV_CLIENT_SIGNING_KEY, configured_path) {
            let client_id = pick(ENV_CLIENT_ID, sync.and_then(|s| s.client_id.as_ref()));
            let tenant_id = pick(ENV_TENANT_ID, sync.and_then(|s| s.tenant_id.as_ref()));
            let kid = pick(ENV_CLIENT_KID, sync.and_then(|s| s.client_kid.as_ref()));
            let audience = pick(
                ENV_ASSERTION_AUDIENCE,
                sync.and_then(|s| s.audience.as_ref()),
            );
            return client_key_auth(&ClientKeySettings {
                signing_key: &signing_key,
                client_id: client_id.as_deref(),
                tenant_id: tenant_id.as_deref(),
                kid: kid.as_deref(),
                audience: audience.as_deref(),
            });
        }

        Ok(
            match self.api_key().map(str::trim).filter(|k| !k.is_empty()) {
                Some(key) => RemoteAuth::ApiKey(key.to_string()),
                None => RemoteAuth::None,
            },
        )
    }
}

impl Default for ChronisConfig {
    fn default() -> Self {
        Self {
            mode: CoreMode::Embedded,
            instance_id: uuid::Uuid::new_v4().to_string(),
            sync: None,
        }
    }
}
