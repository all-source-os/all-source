//! ES256 client assertions for a gateway that holds this client's registered
//! public key (`clientkey.registered`). Enabled by the `es256-assertion` feature.

use std::{
    fmt,
    sync::{Mutex, PoisonError},
    time::{SystemTime, UNIX_EPOCH},
};

use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::Serialize;

use crate::{CredentialProvider, Error};

/// Lifetime of each assertion. The gateway refuses anything longer than 60s.
pub const ASSERTION_LIFETIME_SECS: u64 = 60;
/// A cached assertion is replaced this long before it expires.
pub const ASSERTION_REFRESH_SECS: u64 = 15;

#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a str,
    sub: &'a str,
    aud: &'a str,
    iat: u64,
    exp: u64,
    jti: String,
}

/// Mints a 60-second ES256 assertion signed with a P-256 private key and
/// reuses it until 15 seconds before it expires.
///
/// `client_id` becomes `iss`, `tenant` becomes `sub`, `kid` names the
/// registered key (its RFC 7638 thumbprint) and `audience` must equal the
/// gateway's `ALLSOURCE_ASSERTION_AUDIENCE`.
pub struct Es256AssertionProvider {
    key: EncodingKey,
    client_id: String,
    tenant: String,
    kid: String,
    audience: String,
    cached: Mutex<Option<(String, u64)>>,
}

impl fmt::Debug for Es256AssertionProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Es256AssertionProvider")
            .field("client_id", &self.client_id)
            .field("tenant", &self.tenant)
            .field("kid", &self.kid)
            .field("audience", &self.audience)
            .finish_non_exhaustive()
    }
}

fn now_secs() -> Result<u64, Error> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .map_err(|_| Error::Config("system clock is before 1970".into()))
}

impl Es256AssertionProvider {
    /// Build a provider from a PKCS#8 PEM P-256 private key. A key that cannot
    /// sign ES256 is refused here rather than on the first request.
    pub fn from_pkcs8_pem(
        pem: &[u8],
        client_id: impl Into<String>,
        tenant: impl Into<String>,
        kid: impl Into<String>,
        audience: impl Into<String>,
    ) -> Result<Self, Error> {
        let key = EncodingKey::from_ec_pem(pem).map_err(|e| {
            Error::Config(format!("client signing key is not a P-256 PKCS#8 PEM: {e}"))
        })?;
        let provider = Self {
            key,
            client_id: client_id.into(),
            tenant: tenant.into(),
            kid: kid.into(),
            audience: audience.into(),
            cached: Mutex::new(None),
        };
        if provider.client_id.is_empty()
            || provider.tenant.is_empty()
            || provider.kid.is_empty()
            || provider.audience.is_empty()
        {
            return Err(Error::Config(
                "client_id, tenant, kid and audience are all required".into(),
            ));
        }
        provider.mint(now_secs()?)?;
        Ok(provider)
    }

    fn mint(&self, iat: u64) -> Result<(String, u64), Error> {
        let exp = iat + ASSERTION_LIFETIME_SECS;
        let mut header = Header::new(Algorithm::ES256);
        header.kid = Some(self.kid.clone());
        header.typ = Some("JWT".into());
        let claims = Claims {
            iss: &self.client_id,
            sub: &self.tenant,
            aud: &self.audience,
            iat,
            exp,
            jti: uuid::Uuid::new_v4().to_string(),
        };
        let token = encode(&header, &claims, &self.key)
            .map_err(|e| Error::Config(format!("client assertion signing failed: {e}")))?;
        Ok((token, exp))
    }

    /// Mints an uncached assertion issued at `iat` (Unix seconds), for fixtures
    /// that pin the clock.
    #[doc(hidden)]
    pub fn mint_at(&self, iat: u64) -> Result<String, Error> {
        self.mint(iat).map(|(token, _)| token)
    }
}

impl CredentialProvider for Es256AssertionProvider {
    fn bearer(&self) -> Result<String, Error> {
        let now = now_secs()?;
        let mut cached = self.cached.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((token, exp)) = cached.as_ref() {
            if now + ASSERTION_REFRESH_SECS < *exp {
                return Ok(token.clone());
            }
        }
        let (token, exp) = self.mint(now)?;
        *cached = Some((token.clone(), exp));
        Ok(token)
    }
}
