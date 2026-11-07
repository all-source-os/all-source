//! `[sync]` client key settings produce the assertion the gateway verifies.

use std::sync::{Arc, Mutex};

use aws_lc_rs::{
    digest::{SHA256, digest},
    signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair},
};
use base64::prelude::{BASE64_STANDARD, BASE64_URL_SAFE_NO_PAD, Engine};
use chronis::infrastructure::{
    config::{ChronisConfig, SyncConfig},
    http_core_client::HttpCoreClient,
    remote_auth::RemoteAuth,
};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde_json::Value;

// Shared with the Rust SDK and the gateway's asserted_keys_interop_test.go.
const INTEROP_TEST_KEY: &str =
    include_str!("../../../sdks/rust/tests/fixtures/es256_interop_test_key.pem");
const INTEROP_TOKEN: &str =
    include_str!("../../../sdks/rust/tests/fixtures/es256_interop_token.jwt");
const INTEROP_CLIENT: &str = "longhand-control-plane";
const INTEROP_TENANT: &str = "asserted-tenant";

fn segment(token: &str, index: usize) -> Value {
    let part = token.split('.').nth(index).expect("three segments");
    serde_json::from_slice(&BASE64_URL_SAFE_NO_PAD.decode(part).expect("base64url"))
        .expect("JSON segment")
}

fn interop_kid() -> String {
    segment(INTEROP_TOKEN.trim(), 0)["kid"]
        .as_str()
        .expect("fixture kid")
        .to_string()
}

/// The fixture key's RFC 7638 thumbprint and its public half.
fn interop_public_key() -> (String, DecodingKey) {
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
    let public = DecodingKey::from_ec_components(&x, &y).expect("public key");
    (kid, public)
}

fn verify(token: &str, public: &DecodingKey, check_exp: bool) {
    let mut validation = Validation::new(Algorithm::ES256);
    validation.set_audience(&["https://api.all-source.xyz"]);
    validation.validate_exp = check_exp;
    decode::<Value>(token, public, &validation).expect("signed by the fixture key");
}

fn no_env(_: &str) -> Option<String> {
    None
}

fn client_key_config(signing_key_path: &str) -> ChronisConfig {
    ChronisConfig {
        sync: Some(SyncConfig {
            remote_url: Some("https://api.all-source.xyz".into()),
            api_key: Some("ignored-when-a-client-key-is-set".into()),
            client_id: Some(INTEROP_CLIENT.into()),
            tenant_id: Some(INTEROP_TENANT.into()),
            client_kid: Some(interop_kid()),
            signing_key_path: Some(signing_key_path.into()),
            audience: None,
        }),
        ..ChronisConfig::default()
    }
}

fn bearer(auth: &RemoteAuth) -> String {
    auth.bearer().expect("bearer").expect("a token")
}

#[test]
fn a_configured_client_key_mints_the_interop_shape() {
    let dir = tempfile::tempdir().unwrap();
    let key_path = dir.path().join("client.pem");
    std::fs::write(&key_path, INTEROP_TEST_KEY).unwrap();

    let auth = client_key_config(key_path.to_str().unwrap())
        .remote_auth_with_env(no_env)
        .expect("client key auth");
    assert!(matches!(auth, RemoteAuth::Assertion(_)), "{auth:?}");

    let token = bearer(&auth);
    let committed = INTEROP_TOKEN.trim();
    let (kid, public) = interop_public_key();
    assert_eq!(interop_kid(), kid, "fixture kid is the key's thumbprint");
    verify(committed, &public, false);
    verify(&token, &public, true);
    assert_eq!(segment(&token, 0), segment(committed, 0), "header shape");

    let mut minted = segment(&token, 1);
    let mut expected = segment(committed, 1);
    let iat = minted["iat"].as_u64().expect("iat");
    assert_eq!(minted["exp"].as_u64().expect("exp") - iat, 60);
    for claims in [&mut minted, &mut expected] {
        let object = claims.as_object_mut().expect("claims object");
        for volatile in ["iat", "exp"] {
            object.remove(volatile);
        }
        let jti = object.remove("jti");
        assert!(
            jti.as_ref()
                .and_then(Value::as_str)
                .is_some_and(|j| !j.is_empty())
        );
    }
    assert_eq!(minted, expected, "iss, sub and aud match the fixture");
}

#[test]
fn the_environment_overrides_the_config_and_may_carry_the_pem_inline() {
    let config = client_key_config("/nonexistent/client.pem");
    let env = |name: &str| match name {
        "ALLSOURCE_CLIENT_SIGNING_KEY" => Some(INTEROP_TEST_KEY.to_string()),
        "ALLSOURCE_TENANT_ID" => Some("other-tenant".to_string()),
        _ => None,
    };
    let auth = config.remote_auth_with_env(env).expect("client key auth");
    assert_eq!(segment(&bearer(&auth), 1)["sub"], "other-tenant");
}

#[test]
fn a_blank_environment_variable_does_not_hide_the_config() {
    let dir = tempfile::tempdir().unwrap();
    let key_path = dir.path().join("client.pem");
    std::fs::write(&key_path, INTEROP_TEST_KEY).unwrap();
    let config = client_key_config(key_path.to_str().unwrap());

    let auth = config
        .remote_auth_with_env(|_| Some("  ".to_string()))
        .expect("client key auth");
    assert_eq!(segment(&bearer(&auth), 1)["sub"], INTEROP_TENANT);
}

#[test]
fn a_signing_key_without_its_identity_is_refused() {
    let mut config = client_key_config("/unused");
    if let Some(sync) = config.sync.as_mut() {
        sync.client_kid = None;
    }
    let env =
        |name: &str| (name == "ALLSOURCE_CLIENT_SIGNING_KEY").then(|| INTEROP_TEST_KEY.to_string());
    let err = config.remote_auth_with_env(env).expect_err("refused");
    assert!(err.to_string().contains("client_kid"), "{err}");
}

#[test]
fn a_pem_pasted_into_the_tracked_config_is_refused() {
    let config = client_key_config(INTEROP_TEST_KEY);
    let err = config.remote_auth_with_env(no_env).expect_err("refused");
    assert!(err.to_string().contains("signing_key_path"), "{err}");
}

#[test]
fn without_a_client_key_the_api_key_is_sent_and_blank_means_none() {
    let mut config = ChronisConfig {
        sync: Some(SyncConfig {
            remote_url: Some("http://localhost:3900".into()),
            api_key: Some("ask_example".into()),
            ..SyncConfig::default()
        }),
        ..ChronisConfig::default()
    };
    let auth = config.remote_auth_with_env(no_env).unwrap();
    assert_eq!(auth.bearer().unwrap().as_deref(), Some("ask_example"));

    if let Some(sync) = config.sync.as_mut() {
        sync.api_key = Some(" ".into());
    }
    let auth = config.remote_auth_with_env(no_env).unwrap();
    assert!(auth.bearer().unwrap().is_none());
}

#[tokio::test]
async fn every_request_carries_only_an_authorization_bearer() {
    use axum::{Router, http::HeaderMap, routing::get};

    let seen: Arc<Mutex<Vec<HeaderMap>>> = Arc::default();
    let record = Arc::clone(&seen);
    let app = Router::new().route(
        "/health",
        get(move |headers: HeaderMap| {
            let record = Arc::clone(&record);
            async move {
                record.lock().unwrap().push(headers);
                "ok"
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let dir = tempfile::tempdir().unwrap();
    let key_path = dir.path().join("client.pem");
    std::fs::write(&key_path, INTEROP_TEST_KEY).unwrap();
    let auth = client_key_config(key_path.to_str().unwrap())
        .remote_auth_with_env(no_env)
        .unwrap();
    let client = HttpCoreClient::new(&format!("http://{addr}")).with_auth(auth);
    client.health().await.expect("first request");
    client.health().await.expect("second request");

    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    for headers in seen.iter() {
        assert!(headers.get("x-api-key").is_none());
        let value = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .expect("authorization header");
        let token = value.strip_prefix("Bearer ").expect("bearer scheme");
        assert_eq!(segment(token, 0)["alg"], "ES256");
        assert_eq!(segment(token, 1)["iss"], INTEROP_CLIENT);
    }
}

#[test]
fn init_refuses_a_pem_passed_as_the_signing_key() {
    let dir = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cn"))
        .args(["init", "--remote", "https://api.all-source.xyz"])
        .args(["--client-id", INTEROP_CLIENT, "--tenant-id", INTEROP_TENANT])
        .args(["--client-kid", &interop_kid()])
        .arg(format!("--signing-key={INTEROP_TEST_KEY}"))
        .current_dir(dir.path())
        .env_clear()
        .env("RUST_LOG", "off")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("spawn cn");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--signing-key takes a path"));
    assert!(!dir.path().join(".chronis/config.toml").exists());
}
