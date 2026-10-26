//! Conditional GET for latest-state reads (`docs/contracts/latest-state`).
//!
//! The tag is derived from the response body, never from a store counter:
//! `entity_versions` is keyed by entity id across tenants, so a counter-based
//! tag would let one tenant observe another tenant's writes.
use crate::error::Result;
use axum::{
    body::Body,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::Response,
};
use sha2::{Digest, Sha256};

/// Strong entity tag for `body`: `"sha256:<first 32 hex of sha256(body)>"`,
/// quotes included.
pub fn etag_for(body: &[u8]) -> String {
    let digest = hex::encode(Sha256::digest(body));
    format!("\"sha256:{}\"", &digest[..32])
}

/// Serialize `value` and answer `304 Not Modified` when `If-None-Match`
/// already names its tag, otherwise `200` with the JSON body. Both carry
/// `ETag`.
pub fn conditional_json<T: serde::Serialize>(headers: &HeaderMap, value: &T) -> Result<Response> {
    let body = serde_json::to_vec(value)?;
    let etag = etag_for(&body);
    let etag_header = HeaderValue::from_str(&etag).expect("hex etag is a valid header value");

    if if_none_match_matches(headers, &etag) {
        return Ok(Response::builder()
            .status(StatusCode::NOT_MODIFIED)
            .header(header::ETAG, etag_header)
            .body(Body::empty())
            .expect("static 304 response"));
    }

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ETAG, etag_header)
        .body(Body::from(body))
        .expect("static 200 response"))
}

/// RFC 9110 13.1.2: `*` matches any current representation, otherwise any
/// listed tag matches under weak comparison, so a `W/` prefix is ignored.
fn if_none_match_matches(headers: &HeaderMap, etag: &str) -> bool {
    headers
        .get_all(header::IF_NONE_MATCH)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .any(|candidate| {
            candidate == "*" || candidate.strip_prefix("W/").unwrap_or(candidate) == etag
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    fn headers_with(if_none_match: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::IF_NONE_MATCH,
            HeaderValue::from_str(if_none_match).unwrap(),
        );
        headers
    }

    #[test]
    fn etag_is_quoted_sha256_prefix_of_32_hex() {
        let tag = etag_for(b"{}");
        let digest = hex::encode(Sha256::digest(b"{}"));

        assert_eq!(tag, format!("\"sha256:{}\"", &digest[..32]));
        assert_eq!(tag.len(), 1 + 7 + 32 + 1);
    }

    #[test]
    fn etag_differs_when_body_differs() {
        assert_ne!(etag_for(b"{\"a\":1}"), etag_for(b"{\"a\":2}"));
    }

    #[tokio::test]
    async fn no_if_none_match_returns_200_json_with_etag() {
        let value = serde_json::json!({"a": 1});
        let response = conditional_json(&HeaderMap::new(), &value).unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
        let expected = etag_for(&serde_json::to_vec(&value).unwrap());
        assert_eq!(response.headers()[header::ETAG], expected.as_str());
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            body.as_ref(),
            serde_json::to_vec(&value).unwrap().as_slice()
        );
    }

    #[tokio::test]
    async fn matching_if_none_match_returns_304_with_etag_and_no_body() {
        let value = serde_json::json!({"a": 1});
        let tag = etag_for(&serde_json::to_vec(&value).unwrap());

        let response = conditional_json(&headers_with(&tag), &value).unwrap();

        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(response.headers()[header::ETAG], tag.as_str());
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert!(body.is_empty());
    }

    #[test]
    fn stale_if_none_match_returns_200() {
        let value = serde_json::json!({"a": 1});
        let stale = etag_for(b"{\"a\":0}");

        let response = conditional_json(&headers_with(&stale), &value).unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn list_containing_the_tag_returns_304() {
        let value = serde_json::json!({"a": 1});
        let tag = etag_for(&serde_json::to_vec(&value).unwrap());
        let list = format!("\"sha256:00000000000000000000000000000000\", {tag}");

        let response = conditional_json(&headers_with(&list), &value).unwrap();

        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
    }

    #[test]
    fn star_returns_304() {
        let response = conditional_json(&headers_with("*"), &serde_json::json!({})).unwrap();

        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
    }

    #[test]
    fn weak_form_of_the_tag_returns_304() {
        let value = serde_json::json!({"a": 1});
        let tag = etag_for(&serde_json::to_vec(&value).unwrap());

        let response = conditional_json(&headers_with(&format!("W/{tag}")), &value).unwrap();

        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
    }
}
