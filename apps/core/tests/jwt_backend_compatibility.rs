#![cfg(feature = "server")]

use allsource_core::infrastructure::security::{AuthManager, Claims, Role};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Duration;
use hmac::{Hmac, Mac};
use sha2::Sha256;

fn signed_token(claims: &Claims, secret: &[u8], algorithm: &str) -> String {
    let header = URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(&serde_json::json!({"alg": algorithm, "typ": "JWT"})).unwrap());
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(claims).unwrap());
    let input = format!("{header}.{payload}");
    let mut signer = Hmac::<Sha256>::new_from_slice(secret).unwrap();
    signer.update(input.as_bytes());
    format!(
        "{input}.{}",
        URL_SAFE_NO_PAD.encode(signer.finalize().into_bytes())
    )
}

#[test]
fn jwt_backend_preserves_hs256_compatibility_and_rejects_invalid_tokens() {
    let secret = b"synthetic-jwt-compatibility-secret";
    let auth = AuthManager::new(std::str::from_utf8(secret).unwrap());
    let claims = Claims::new(
        "synthetic-user".into(),
        "synthetic-tenant".into(),
        Role::Developer,
        Duration::minutes(5),
    );
    let validated = auth
        .validate_token(&signed_token(&claims, secret, "HS256"))
        .unwrap();
    assert_eq!(validated.tenant_id, "synthetic-tenant");
    assert_eq!(validated.role, Role::Developer);
    assert!(
        auth.validate_token(&signed_token(&claims, b"wrong-secret", "HS256"))
            .is_err()
    );
    assert!(
        auth.validate_token(&signed_token(&claims, secret, "HS384"))
            .is_err()
    );
    let expired = Claims::new(
        "synthetic-user".into(),
        "synthetic-tenant".into(),
        Role::Developer,
        Duration::minutes(-5),
    );
    assert!(
        auth.validate_token(&signed_token(&expired, secret, "HS256"))
            .is_err()
    );
}
