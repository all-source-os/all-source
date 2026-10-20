//! Google service-account access tokens (the JWT-bearer grant).
//!
//! Search Console and the GA4 Data API both accept a service account, which is
//! the only Google auth flow that works unattended: the installed-app flow
//! needs a browser and a refresh token that expires when the consent screen
//! stays in testing mode.
//!
//! Granting the key access is a separate step from creating it, and it is the
//! one that is always forgotten — the service account's own email must be
//! added as a user on the Search Console property and on the GA4 property.
//! See the runbook.

use anyhow::{Context, Result, bail};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};

const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:jwt-bearer";

#[derive(Debug, Deserialize)]
struct ServiceAccountKey {
    client_email: String,
    private_key: String,
}

#[derive(Debug, Serialize)]
struct Assertion<'a> {
    iss: &'a str,
    scope: &'a str,
    aud: &'a str,
    exp: i64,
    iat: i64,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
}

/// Exchanges a service-account key for an access token scoped to `scope`.
///
/// `key_json` is the raw JSON of the key file, which is how it arrives from a
/// CI secret. The assertion is signed RS256 and lives for one hour, Google's
/// maximum; there is no refresh because the next run mints a new one.
pub async fn access_token(client: &reqwest::Client, key_json: &str, scope: &str) -> Result<String> {
    let key: ServiceAccountKey = serde_json::from_str(key_json)
        .context("service-account key is not valid JSON — paste the whole downloaded file")?;

    let now = chrono::Utc::now().timestamp();
    let assertion = Assertion {
        iss: &key.client_email,
        scope,
        aud: TOKEN_URL,
        exp: now + 3600,
        iat: now,
    };

    let encoding_key = EncodingKey::from_rsa_pem(key.private_key.as_bytes())
        .context("service-account private_key is not a PEM RSA key")?;
    let jwt = jsonwebtoken::encode(&Header::new(Algorithm::RS256), &assertion, &encoding_key)
        .context("signing the service-account assertion failed")?;

    let response = client
        .post(TOKEN_URL)
        .form(&[("grant_type", GRANT_TYPE), ("assertion", &jwt)])
        .send()
        .await
        .context("token endpoint unreachable")?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("token exchange failed: HTTP {status}: {body}");
    }

    let token: TokenResponse =
        serde_json::from_str(&body).context("token response was not the expected shape")?;
    Ok(token.access_token)
}
