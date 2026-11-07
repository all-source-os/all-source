//! A registered client key signs every gateway request with the assertion the
//! gateway verifies.

use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    thread,
};

use allsource::Es256AssertionProvider;
use aws_lc_rs::{
    digest::{SHA256, digest},
    signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair},
};
use base64::prelude::{BASE64_STANDARD, BASE64_URL_SAFE_NO_PAD, Engine};
use geo_core::GeoConfig;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde_json::Value;

// Shared with the Rust SDK and the gateway's asserted_keys_interop_test.go.
const INTEROP_TEST_KEY: &str =
    include_str!("../../../../sdks/rust/tests/fixtures/es256_interop_test_key.pem");
const INTEROP_TOKEN: &str =
    include_str!("../../../../sdks/rust/tests/fixtures/es256_interop_token.jwt");
const INTEROP_CLIENT: &str = "longhand-control-plane";
const INTEROP_TENANT: &str = "asserted-tenant";
const AUDIENCE: &str = "https://api.all-source.xyz";

fn segment(token: &str, index: usize) -> Value {
    let part = token.split('.').nth(index).expect("three segments");
    serde_json::from_slice(&BASE64_URL_SAFE_NO_PAD.decode(part).expect("base64url"))
        .expect("JSON segment")
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
    validation.set_audience(&[AUDIENCE]);
    validation.validate_exp = check_exp;
    decode::<Value>(token, public, &validation).expect("signed by the fixture key");
}

/// One request's headers as (lower-cased name, value).
type Headers = Vec<(String, String)>;

/// Answers `requests` health checks, one per connection, and returns the
/// headers of each request.
fn health_server(requests: usize) -> (String, thread::JoinHandle<Vec<Headers>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!("http://{}", listener.local_addr().expect("addr"));
    let handle = thread::spawn(move || {
        let mut seen = Vec::new();
        for stream in listener.incoming().take(requests) {
            let mut stream = stream.expect("connection");
            let mut headers = Vec::new();
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).expect("request line");
                let line = line.trim_end().to_string();
                if line.is_empty() {
                    break;
                }
                headers.push(match line.split_once(':') {
                    Some((name, value)) => (name.to_ascii_lowercase(), value.trim().to_string()),
                    None => (line, String::new()),
                });
            }
            let body = r#"{"status":"ok"}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\
                 connection: close\r\n\r\n{body}",
                body.len()
            )
            .expect("response");
            seen.push(headers);
        }
        seen
    });
    (url, handle)
}

#[test]
fn the_client_key_mints_the_interop_shape() {
    let committed = INTEROP_TOKEN.trim();
    let (kid, public) = interop_public_key();
    assert_eq!(segment(committed, 0)["kid"], kid.as_str());
    verify(committed, &public, false);

    let provider = Es256AssertionProvider::from_pkcs8_pem(
        INTEROP_TEST_KEY.as_bytes(),
        INTEROP_CLIENT,
        INTEROP_TENANT,
        &kid,
        AUDIENCE,
    )
    .expect("provider");
    let token = allsource::CredentialProvider::bearer(&provider).expect("bearer");
    verify(&token, &public, true);
    assert_eq!(segment(&token, 0), segment(committed, 0), "header shape");
    let mut minted = segment(&token, 1);
    let mut expected = segment(committed, 1);
    for claims in [&mut minted, &mut expected] {
        let object = claims.as_object_mut().expect("claims object");
        for volatile in ["iat", "exp", "jti"] {
            object.remove(volatile);
        }
    }
    assert_eq!(minted, expected, "iss, sub and aud match the fixture");
}

#[tokio::test]
async fn every_client_from_one_config_sends_only_the_shared_assertion() {
    let (kid, public) = interop_public_key();
    let provider = Es256AssertionProvider::from_pkcs8_pem(
        INTEROP_TEST_KEY.as_bytes(),
        INTEROP_CLIENT,
        INTEROP_TENANT,
        &kid,
        AUDIENCE,
    )
    .expect("provider");
    let (url, server) = health_server(2);
    let config = GeoConfig::with_assertion(&url, provider);
    assert!(config.api_key().is_none());

    config
        .core_client()
        .expect("core client")
        .health()
        .await
        .expect("core health");
    config
        .query_client()
        .expect("query client")
        .health()
        .await
        .expect("query health");

    let seen = server.join().expect("server thread");
    assert_eq!(seen.len(), 2);
    let mut tokens = Vec::new();
    for headers in &seen {
        assert!(
            !headers.iter().any(|(name, _)| name == "x-api-key"),
            "{headers:?}"
        );
        let token = headers
            .iter()
            .find(|(name, _)| name == "authorization")
            .and_then(|(_, value)| value.strip_prefix("Bearer "))
            .expect("authorization bearer");
        verify(token, &public, true);
        assert_eq!(segment(token, 1)["sub"], INTEROP_TENANT);
        tokens.push(token.to_string());
    }
    assert_eq!(tokens[0], tokens[1], "one provider, one cached assertion");
}
