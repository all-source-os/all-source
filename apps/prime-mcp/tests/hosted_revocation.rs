use std::time::Duration;

use reqwest::{Client, StatusCode, Url};
use serde_json::Value;

#[tokio::test]
#[ignore = "requires an already revoked private proof credential and explicit --ignored"]
async fn revoked_hosted_proof_key_is_rejected() {
    tokio::time::timeout(Duration::from_secs(20), async {
        assert!(std::env::args().any(|arg| arg == "--ignored"));
        let endpoint = std::env::var("PRIME_SYNC_TO").expect("hosted origin required");
        let mut url = Url::parse(&endpoint).expect("hosted origin URL");
        assert_eq!(url.scheme(), "https");
        assert!(url.username().is_empty() && url.password().is_none());
        assert!(url.query().is_none() && url.fragment().is_none());
        assert_eq!(url.path(), "/");
        url.set_path("/api/v1/events/query");
        let receipt =
            std::env::var("ALLSTACK_HOSTED_PROOF_RECEIPT").expect("proof receipt required");
        let proof: Value = serde_json::from_slice(&std::fs::read(receipt).expect("read proof"))
            .expect("proof JSON");
        url.query_pairs_mut().append_pair(
            "entity_id",
            proof["proofs"][0]["entity_id"]
                .as_str()
                .expect("proof entity"),
        );
        let key_file = std::env::var("ALLSTACK_HOSTED_PROOF_KEY_FILE")
            .expect("private revoked credential file required");
        let key = std::fs::read_to_string(key_file).expect("read private credential");
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .expect("bounded hosted client");
        let response = client
            .get(url)
            .bearer_auth(key)
            .send()
            .await
            .expect("hosted response");
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "revoked proof credential must not authorize hosted event reads"
        );
    })
    .await
    .expect("revocation check exceeded single deadline");
}
