use std::sync::{Arc, OnceLock};

use allsource::{
    ClientConfig, CoreClient, CredentialProvider, Es256AssertionProvider, QueryClient,
};

use crate::error::GeoError;

/// The Control Plane gateway. GEO tooling never talks to Core directly —
/// public auth terminates at the gateway.
pub const DEFAULT_API_URL: &str = "https://api.all-source.xyz";

/// Environment variable holding the gateway base URL.
pub const ENV_API_URL: &str = "ALLSOURCE_API_URL";

/// Environment variable holding the gateway API key.
pub const ENV_API_KEY: &str = "ALLSOURCE_API_KEY";

/// Environment variable holding a registered client key's P-256 PKCS#8 private
/// key, as a PEM or a path to one. When set it replaces `ALLSOURCE_API_KEY`.
pub const ENV_CLIENT_SIGNING_KEY: &str = "ALLSOURCE_CLIENT_SIGNING_KEY";

/// The registered client id; the assertion's `iss`.
pub const ENV_CLIENT_ID: &str = "ALLSOURCE_CLIENT_ID";

/// The tenant the client key is registered on; the assertion's `sub`.
pub const ENV_TENANT_ID: &str = "ALLSOURCE_TENANT_ID";

/// The registered key's kid (its RFC 7638 thumbprint).
pub const ENV_CLIENT_KID: &str = "ALLSOURCE_CLIENT_KID";

/// The audience the gateway expects; defaults to [`DEFAULT_API_URL`].
pub const ENV_ASSERTION_AUDIENCE: &str = "ALLSOURCE_ASSERTION_AUDIENCE";

/// What happened when the process looked for a `.env` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DotenvOutcome {
    /// A `.env` was found and its entries were offered to the environment.
    Loaded {
        /// Absolute path of the file that was read.
        path: String,
    },
    /// No `.env` anywhere up the tree. Not an error: the keyless dry-run path
    /// and CI both run this way.
    NotFound,
    /// A `.env` exists but could not be read or parsed. Reported, never fatal —
    /// a malformed `.env` must not take down a run whose keys are already in
    /// the real environment.
    Unreadable {
        /// Why it could not be used.
        reason: String,
    },
}

/// The single `.env` load site for the whole of `geo-core`.
static DOTENV: OnceLock<DotenvOutcome> = OnceLock::new();

/// Load `.env` once, before any variable is read.
///
/// **Real process environment variables always win.** The loader below sets
/// only variables that are not already present, and it is the *only* dotenvy
/// call in the crate — there is deliberately no overriding variant anywhere,
/// and a test asserts that. Getting this backwards would let a stale `.env`
/// silently shadow a deliberate inline override (`ANTHROPIC_API_KEY=... geo
/// probe`) during a live sweep, which is exactly the kind of failure that
/// wastes an afternoon and a sweep's worth of spend.
///
/// dotenvy searches upward from the current directory, so the repository-root
/// `.env` is found whether you run from the repo root or from `tooling/geo`.
///
/// Idempotent: the first call does the work, later calls return the same
/// outcome. Callers may invoke it freely; the CLI calls it once at startup so
/// the outcome can be printed.
pub fn init_env() -> &'static DotenvOutcome {
    DOTENV.get_or_init(|| match dotenvy::dotenv() {
        Ok(path) => DotenvOutcome::Loaded {
            path: path.display().to_string(),
        },
        Err(e) if e.not_found() => DotenvOutcome::NotFound,
        Err(e) => DotenvOutcome::Unreadable {
            reason: e.to_string(),
        },
    })
}

/// The credential sent to the gateway.
#[derive(Clone)]
enum GeoCredential {
    /// A long-lived API key sent unchanged.
    ApiKey(String),
    /// A registered client key; the SDK asks it for a fresh assertion on every
    /// request attempt.
    Assertion(Arc<Es256AssertionProvider>),
}

/// Where GEO telemetry is written and with what credential.
#[derive(Clone)]
pub struct GeoConfig {
    api_url: String,
    credential: GeoCredential,
}

impl GeoConfig {
    /// Build a config explicitly (tests, or a caller that already has a key).
    pub fn new(api_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            api_url: api_url.into(),
            credential: GeoCredential::ApiKey(api_key.into()),
        }
    }

    /// Build a config that signs every request with a registered client key.
    pub fn with_assertion(api_url: impl Into<String>, provider: Es256AssertionProvider) -> Self {
        Self {
            api_url: api_url.into(),
            credential: GeoCredential::Assertion(Arc::new(provider)),
        }
    }

    /// Read `ALLSOURCE_API_URL` (optional) and a credential: the
    /// `ALLSOURCE_CLIENT_*` registered client key when
    /// `ALLSOURCE_CLIENT_SIGNING_KEY` is set, otherwise `ALLSOURCE_API_KEY`.
    ///
    /// A missing credential is a hard error with an actionable message — see
    /// [`GeoError::MissingApiKey`]. There is no silent no-op mode; use
    /// [`crate::EmitMode::DryRun`] when you deliberately do not want to write.
    pub fn from_env() -> Result<Self, GeoError> {
        // Cheap and idempotent, so config resolution is correct no matter
        // which entry point reached it.
        init_env();

        let api_url = match std::env::var(ENV_API_URL) {
            Ok(url) if url.trim().is_empty() => return Err(GeoError::EmptyApiUrl),
            Ok(url) => url.trim().trim_end_matches('/').to_string(),
            Err(_) => DEFAULT_API_URL.to_string(),
        };

        if let Some(signing_key) = env_nonblank(ENV_CLIENT_SIGNING_KEY) {
            let provider = assertion_from_env(&signing_key)?;
            return Ok(Self::with_assertion(api_url, provider));
        }

        let api_key = env_nonblank(ENV_API_KEY).ok_or(GeoError::MissingApiKey)?;

        Ok(Self::new(api_url, api_key))
    }

    /// Gateway base URL (no trailing slash).
    pub fn api_url(&self) -> &str {
        &self.api_url
    }

    /// Gateway API key, when the credential is one.
    pub fn api_key(&self) -> Option<&str> {
        match &self.credential {
            GeoCredential::ApiKey(key) => Some(key),
            GeoCredential::Assertion(_) => None,
        }
    }

    /// An ingest-capable client pointed at the gateway.
    pub fn core_client(&self) -> Result<CoreClient, allsource::Error> {
        match &self.credential {
            GeoCredential::ApiKey(key) => CoreClient::new(&self.api_url, key),
            GeoCredential::Assertion(provider) => CoreClient::with_config_and_credential_provider(
                ClientConfig::new(&self.api_url, ""),
                shared(provider),
            ),
        }
    }

    /// A query client pointed at the gateway.
    pub fn query_client(&self) -> Result<QueryClient, allsource::Error> {
        match &self.credential {
            GeoCredential::ApiKey(key) => QueryClient::new(&self.api_url, key),
            GeoCredential::Assertion(provider) => QueryClient::with_config_and_credential_provider(
                ClientConfig::new(&self.api_url, ""),
                shared(provider),
            ),
        }
    }
}

/// One provider, and so one cached assertion, behind every client built from a config.
fn shared(provider: &Arc<Es256AssertionProvider>) -> impl CredentialProvider + 'static {
    let provider = Arc::clone(provider);
    move || provider.bearer()
}

fn env_nonblank(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Build the assertion provider from the `ALLSOURCE_CLIENT_*` variables.
fn assertion_from_env(signing_key: &str) -> Result<Es256AssertionProvider, GeoError> {
    let (Some(client_id), Some(tenant_id), Some(kid)) = (
        env_nonblank(ENV_CLIENT_ID),
        env_nonblank(ENV_TENANT_ID),
        env_nonblank(ENV_CLIENT_KID),
    ) else {
        return Err(GeoError::ClientKey {
            reason: "ALLSOURCE_CLIENT_SIGNING_KEY needs ALLSOURCE_CLIENT_ID, \
                     ALLSOURCE_TENANT_ID and ALLSOURCE_CLIENT_KID"
                .to_string(),
        });
    };
    let audience =
        env_nonblank(ENV_ASSERTION_AUDIENCE).unwrap_or_else(|| DEFAULT_API_URL.to_string());
    let pem = if signing_key.contains("-----BEGIN") {
        signing_key.as_bytes().to_vec()
    } else {
        std::fs::read(signing_key).map_err(|e| GeoError::ClientKey {
            reason: format!(
                "ALLSOURCE_CLIENT_SIGNING_KEY is neither a PEM nor a readable file: {e}"
            ),
        })?
    };
    let built = Es256AssertionProvider::from_pkcs8_pem(&pem, client_id, tenant_id, kid, audience);
    built.map_err(|e| GeoError::ClientKey {
        reason: e.to_string(),
    })
}

/// Redacts the credential so a `{config:?}` in a log line can never leak it.
impl std::fmt::Debug for GeoConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let credential = match self.credential {
            GeoCredential::ApiKey(_) => "api key <redacted>",
            GeoCredential::Assertion(_) => "registered client key",
        };
        f.debug_struct("GeoConfig")
            .field("api_url", &self.api_url)
            .field("credential", &credential)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_real_environment_variable_beats_a_dotenv_entry() {
        // The precedence that matters: `ANTHROPIC_API_KEY=... geo probe` must
        // win over a stale repository-root `.env`, or a live sweep silently
        // authenticates as the wrong thing.
        //
        // `CARGO_PKG_NAME` is guaranteed to already be in this process's real
        // environment (cargo sets it), so the test needs no `set_var` — which
        // is unsafe in this edition and denied workspace-wide.
        let dir = std::env::temp_dir().join(format!(
            "geo-dotenv-precedence-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join(".env");
        std::fs::write(
            &path,
            "CARGO_PKG_NAME=value-from-dotenv\nGEO_DOTENV_TEST_ONLY=value-from-dotenv\n",
        )
        .expect("write .env");

        dotenvy::from_filename(&path).expect("the fixture .env loads");

        assert_eq!(
            std::env::var("CARGO_PKG_NAME").as_deref(),
            Ok(env!("CARGO_PKG_NAME")),
            "a .env entry overwrote a real process environment variable"
        );
        assert_eq!(
            std::env::var("GEO_DOTENV_TEST_ONLY").as_deref(),
            Ok("value-from-dotenv"),
            "a .env entry with no real counterpart was not applied"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn there_is_exactly_one_load_site_and_it_does_not_override() {
        // Two properties, asserted on the source because the difference is
        // invisible at runtime until the one run where it matters:
        //   1. `dotenv()` is the non-overriding loader; every `*_override`
        //      variant would let a stale .env shadow a real env var.
        //   2. Exactly one call site, in this file. Scattered loads are how
        //      load order becomes accidental.
        let source = include_str!("config.rs");
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        let calls: Vec<&str> = production
            .match_indices("dotenvy::")
            .map(|(i, _)| {
                let rest = &production[i..];
                &rest[..rest.len().min(24)]
            })
            .collect();
        assert_eq!(calls.len(), 1, "expected one dotenvy call, found {calls:?}");
        assert!(
            calls[0].starts_with("dotenvy::dotenv()"),
            "unexpected loader: {}",
            calls[0]
        );

        // ...and nowhere else in the crate.
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for entry in std::fs::read_dir(&src).expect("src dir") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs")
                || path.file_name().and_then(|n| n.to_str()) == Some("config.rs")
            {
                continue;
            }
            let body = std::fs::read_to_string(&path).expect("read source");
            assert!(
                !body.contains("dotenvy"),
                "{} calls dotenvy directly; go through config::init_env instead",
                path.display()
            );
        }
    }

    #[test]
    fn a_missing_dotenv_is_not_an_error() {
        // The keyless dry-run path and CI both run with no .env at all.
        let outcome = init_env();
        assert!(
            matches!(
                outcome,
                DotenvOutcome::Loaded { .. } | DotenvOutcome::NotFound
            ),
            "{outcome:?}"
        );
    }
}
