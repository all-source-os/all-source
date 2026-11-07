//! The credential sync sends to the hosted gateway: a tenant API key, or a
//! fresh ES256 assertion from a registered client key.
//!
//! `bearer()` is called for every request, so an assertion is never sent past
//! its 60-second lifetime.

use std::{fmt, sync::Arc};

use allsource::{CredentialProvider, Es256AssertionProvider};
use anyhow::{Context, Result};

/// How sync requests to the remote authenticate.
#[derive(Clone)]
pub enum RemoteAuth {
    /// A long-lived tenant API key sent unchanged.
    ApiKey(String),
    /// A registered client key; each call may mint a new assertion.
    Assertion(Arc<dyn CredentialProvider>),
}

impl RemoteAuth {
    /// The bearer token for one request, without the `Bearer ` prefix.
    pub fn bearer(&self) -> Result<String> {
        match self {
            Self::ApiKey(key) => Ok(key.clone()),
            Self::Assertion(provider) => provider
                .bearer()
                .map_err(|e| anyhow::anyhow!("client assertion: {e}")),
        }
    }
}

impl fmt::Debug for RemoteAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ApiKey(_) => f.write_str("RemoteAuth::ApiKey(<redacted>)"),
            Self::Assertion(_) => f.write_str("RemoteAuth::Assertion"),
        }
    }
}

/// Settings for signing gateway requests with a registered client key.
pub struct AssertionSettings<'a> {
    /// A P-256 PKCS#8 PEM, or a path to one.
    pub signing_key: &'a str,
    pub client_id: Option<&'a str>,
    pub tenant_id: Option<&'a str>,
    pub kid: Option<&'a str>,
    pub audience: &'a str,
}

/// Build the assertion credential. Every field but the audience is required,
/// and an unreadable or non-P-256 key fails here instead of on the first push.
pub fn assertion(settings: &AssertionSettings<'_>) -> Result<RemoteAuth> {
    let (Some(client_id), Some(tenant_id), Some(kid)) =
        (settings.client_id, settings.tenant_id, settings.kid)
    else {
        anyhow::bail!(
            "--client-signing-key needs --client-id, --client-tenant-id and --client-kid \
             (ALLSOURCE_CLIENT_ID, ALLSOURCE_TENANT_ID, ALLSOURCE_CLIENT_KID)"
        );
    };
    let pem = read_signing_key(settings.signing_key)?;
    let audience = settings.audience;
    let built = Es256AssertionProvider::from_pkcs8_pem(&pem, client_id, tenant_id, kid, audience);
    let provider = built.map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(RemoteAuth::Assertion(Arc::new(provider)))
}

/// Inline PEM, or a path to one.
fn read_signing_key(source: &str) -> Result<Vec<u8>> {
    if source.contains("-----BEGIN") {
        return Ok(source.as_bytes().to_vec());
    }
    std::fs::read(source)
        .context("ALLSOURCE_CLIENT_SIGNING_KEY is neither a PEM nor a readable file")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Shared with the Rust SDK and the gateway's asserted_keys_interop_test.go.
    const INTEROP_TEST_KEY: &str =
        include_str!("../../../sdks/rust/tests/fixtures/es256_interop_test_key.pem");
    const INTEROP_TOKEN: &str =
        include_str!("../../../sdks/rust/tests/fixtures/es256_interop_token.jwt");
    const AUDIENCE: &str = "https://api.all-source.xyz";

    fn segment(token: &str, index: usize) -> serde_json::Value {
        use base64::prelude::{BASE64_URL_SAFE_NO_PAD, Engine};
        let part = token.split('.').nth(index).expect("three segments");
        serde_json::from_slice(&BASE64_URL_SAFE_NO_PAD.decode(part).expect("base64url"))
            .expect("JSON segment")
    }

    /// The fixture key's RFC 7638 thumbprint and its public half.
    fn interop_public_key() -> (String, jsonwebtoken::DecodingKey) {
        use aws_lc_rs::{
            digest::{SHA256, digest},
            signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair},
        };
        use base64::prelude::{BASE64_STANDARD, BASE64_URL_SAFE_NO_PAD, Engine};

        let body: String = INTEROP_TEST_KEY
            .lines()
            .filter(|line| !line.starts_with("-----"))
            .collect();
        let der = BASE64_STANDARD.decode(body).expect("fixture PEM body");
        let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &der)
            .expect("fixture is a P-256 PKCS#8 key");
        let point = pair.public_key().as_ref();
        let x = BASE64_URL_SAFE_NO_PAD.encode(&point[1..33]);
        let y = BASE64_URL_SAFE_NO_PAD.encode(&point[33..]);
        let canonical = format!(r#"{{"crv":"P-256","kty":"EC","x":"{x}","y":"{y}"}}"#);
        let kid = BASE64_URL_SAFE_NO_PAD.encode(digest(&SHA256, canonical.as_bytes()).as_ref());
        let public = jsonwebtoken::DecodingKey::from_ec_components(&x, &y).expect("public key");
        (kid, public)
    }

    fn verify(token: &str, public: &jsonwebtoken::DecodingKey, check_exp: bool) {
        let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::ES256);
        validation.set_audience(&[AUDIENCE]);
        validation.validate_exp = check_exp;
        jsonwebtoken::decode::<serde_json::Value>(token, public, &validation)
            .expect("signed by the fixture key");
    }

    #[test]
    fn the_client_key_mints_the_interop_shape() {
        let committed = INTEROP_TOKEN.trim();
        let (kid, public) = interop_public_key();
        assert_eq!(segment(committed, 0)["kid"], kid.as_str());
        verify(committed, &public, false);

        let auth = assertion(&AssertionSettings {
            signing_key: INTEROP_TEST_KEY,
            client_id: Some("longhand-control-plane"),
            tenant_id: Some("asserted-tenant"),
            kid: Some(&kid),
            audience: AUDIENCE,
        })
        .expect("client key auth");
        let token = auth.bearer().expect("bearer");
        verify(&token, &public, true);
        assert_eq!(segment(&token, 0), segment(committed, 0), "header shape");

        let mut minted = segment(&token, 1);
        let mut expected = segment(committed, 1);
        let iat = minted["iat"].as_u64().expect("iat");
        assert_eq!(minted["exp"].as_u64().expect("exp") - iat, 60);
        for claims in [&mut minted, &mut expected] {
            let object = claims.as_object_mut().expect("claims object");
            object.remove("iat");
            object.remove("exp");
            let jti = object.remove("jti");
            assert!(
                jti.as_ref()
                    .and_then(|j| j.as_str())
                    .is_some_and(|j| !j.is_empty())
            );
        }
        assert_eq!(minted, expected, "iss, sub and aud match the fixture");
    }

    #[test]
    fn a_signing_key_without_its_identity_is_refused() {
        let err = assertion(&AssertionSettings {
            signing_key: INTEROP_TEST_KEY,
            client_id: Some("client"),
            tenant_id: None,
            kid: Some("kid"),
            audience: "https://api.all-source.xyz",
        })
        .expect_err("refused");
        assert!(err.to_string().contains("--client-tenant-id"), "{err}");
    }

    #[test]
    fn an_unreadable_key_path_is_refused_up_front() {
        let err = assertion(&AssertionSettings {
            signing_key: "/nonexistent/client-key.pem",
            client_id: Some("client"),
            tenant_id: Some("tenant"),
            kid: Some("kid"),
            audience: "https://api.all-source.xyz",
        })
        .expect_err("refused");
        assert!(err.to_string().contains("readable file"), "{err}");
    }

    #[test]
    fn the_api_key_is_redacted_from_debug_output() {
        let auth = RemoteAuth::ApiKey("ask_secret".into());
        assert!(!format!("{auth:?}").contains("ask_secret"));
    }
}
