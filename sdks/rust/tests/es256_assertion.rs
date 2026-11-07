//! The ES256 assertion provider mints what the gateway verifies.
#![cfg(feature = "es256-assertion")]

use allsource::{CredentialProvider, Error, Es256AssertionProvider};
use aws_lc_rs::{
    digest::{digest, SHA256},
    rand::SystemRandom,
    signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_FIXED_SIGNING},
};
use base64::prelude::{Engine, BASE64_STANDARD, BASE64_URL_SAFE_NO_PAD};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde_json::Value;

const AUDIENCE: &str = "https://api.all-source.xyz";

// The gateway's asserted_keys_interop_test.go verifies the same two fixtures,
// with the same client, tenant, audience and issue time.
const INTEROP_TEST_KEY: &str = include_str!("fixtures/es256_interop_test_key.pem");
const INTEROP_TOKEN: &str = include_str!("fixtures/es256_interop_token.jwt");
const INTEROP_CLIENT: &str = "longhand-control-plane";
const INTEROP_TENANT: &str = "asserted-tenant";
const INTEROP_IAT: u64 = 1_794_009_600;
const REGENERATE: &str = "regenerate with `cargo test -p allsource --features es256-assertion \
     --test es256_assertion -- --ignored --nocapture print_interop_token` and re-run the \
     gateway's interop test";

/// The fixture key's RFC 7638 thumbprint and its public key.
fn interop_key() -> (String, DecodingKey) {
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

fn interop_token(kid: &str) -> String {
    Es256AssertionProvider::from_pkcs8_pem(
        INTEROP_TEST_KEY.as_bytes(),
        INTEROP_CLIENT,
        INTEROP_TENANT,
        kid,
        AUDIENCE,
    )
    .expect("provider")
    .mint_at(INTEROP_IAT)
    .expect("mint")
}

fn segment(token: &str, index: usize) -> Value {
    let part = token.split('.').nth(index).expect("three segments");
    serde_json::from_slice(&BASE64_URL_SAFE_NO_PAD.decode(part).expect("base64url"))
        .expect("JSON segment")
}

#[test]
fn the_committed_interop_token_is_what_the_provider_mints_today() {
    let (kid, public) = interop_key();
    let committed = INTEROP_TOKEN.trim();
    let minted = interop_token(&kid);

    assert_eq!(
        segment(committed, 0),
        segment(&minted, 0),
        "header drifted: {REGENERATE}"
    );
    assert_eq!(segment(committed, 0)["kid"], kid.as_str());
    let mut claims = [segment(committed, 1), segment(&minted, 1)];
    for set in &mut claims {
        let jti = set.as_object_mut().expect("claims object").remove("jti");
        assert!(jti
            .as_ref()
            .and_then(Value::as_str)
            .is_some_and(|jti| !jti.is_empty()));
    }
    assert_eq!(claims[0], claims[1], "claims drifted: {REGENERATE}");

    let mut validation = Validation::new(Algorithm::ES256);
    validation.set_audience(&[AUDIENCE]);
    validation.validate_exp = false;
    decode::<Value>(committed, &public, &validation)
        .expect("committed token is signed by the fixture key");
}

#[test]
#[ignore]
fn print_interop_token() {
    let (kid, _) = interop_key();
    println!("{}", interop_token(&kid));
}

fn generated_key() -> (String, DecodingKey) {
    let pkcs8 =
        EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &SystemRandom::new())
            .expect("generate P-256 key");
    let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref())
        .expect("parse generated key");
    let point = pair.public_key().as_ref().to_vec();
    let decoding = DecodingKey::from_ec_components(
        &BASE64_URL_SAFE_NO_PAD.encode(&point[1..33]),
        &BASE64_URL_SAFE_NO_PAD.encode(&point[33..]),
    )
    .expect("public key");
    let body = BASE64_STANDARD.encode(pkcs8.as_ref());
    let lines: Vec<&str> = body
        .as_bytes()
        .chunks(64)
        .map(|line| std::str::from_utf8(line).expect("base64 is ascii"))
        .collect();
    let pem = format!(
        "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
        lines.join("\n")
    );
    (pem, decoding)
}

#[test]
fn the_assertion_carries_the_registered_identity_and_a_60s_lifetime() {
    let (pem, public) = generated_key();
    let provider = Es256AssertionProvider::from_pkcs8_pem(
        pem.as_bytes(),
        "longhand-control-plane",
        "onboard-tenant",
        "kid-1",
        AUDIENCE,
    )
    .expect("provider");

    let token = provider.bearer().expect("token");
    let header = decode_header(&token).expect("header");
    assert_eq!(header.alg, Algorithm::ES256);
    assert_eq!(header.kid.as_deref(), Some("kid-1"));
    assert_eq!(header.typ.as_deref(), Some("JWT"));

    let mut validation = Validation::new(Algorithm::ES256);
    validation.set_audience(&[AUDIENCE]);
    validation.set_issuer(&["longhand-control-plane"]);
    let claims = decode::<Value>(&token, &public, &validation)
        .expect("signature verifies with the public key")
        .claims;
    assert_eq!(claims["sub"], "onboard-tenant");
    assert_eq!(claims["aud"], AUDIENCE, "aud is a single string");
    assert_eq!(
        claims["exp"].as_u64().expect("exp") - claims["iat"].as_u64().expect("iat"),
        60
    );
    assert!(claims["jti"].as_str().is_some_and(|jti| !jti.is_empty()));
    assert!(claims.get("role").is_none() && claims.get("scope").is_none());

    assert_eq!(
        provider.bearer().expect("cached"),
        token,
        "a fresh assertion is reused until 15s before it expires"
    );
}

#[test]
fn a_key_that_is_not_a_pkcs8_ec_pem_is_refused_up_front() {
    let err = Es256AssertionProvider::from_pkcs8_pem(
        b"-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\n",
        "client",
        "tenant",
        "kid",
        AUDIENCE,
    )
    .expect_err("refused");
    assert!(matches!(err, Error::Config(_)), "{err:?}");

    let (pem, _) = generated_key();
    assert!(
        Es256AssertionProvider::from_pkcs8_pem(pem.as_bytes(), "", "tenant", "kid", AUDIENCE)
            .is_err()
    );
}
