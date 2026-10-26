use super::EVENT;
use chrono::{DateTime, Utc};
use reqwest::{Client, Method, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};
use uuid::Uuid;

pub(super) struct Config {
    pub(super) endpoint: String,
    pub(super) key: String,
    pub(super) receipt: PathBuf,
    pub(super) http: Client,
}

impl Config {
    pub(super) fn load() -> Self {
        assert!(
            std::env::args().any(|arg| arg == "--ignored"),
            "explicit --ignored opt-in required"
        );
        let required = |name| {
            let value = std::env::var(name)
                .unwrap_or_else(|_| panic!("required hosted proof environment missing"));
            assert!(
                !value.trim().is_empty(),
                "hosted proof environment is blank"
            );
            value
        };
        let endpoint = required("PRIME_SYNC_TO");
        let url = Url::parse(&endpoint).expect("PRIME_SYNC_TO must be an HTTPS origin");
        assert!(
            url.scheme() == "https"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
                && url.path() == "/",
            "PRIME_SYNC_TO must be an HTTPS origin without credentials or query"
        );
        let key = std::env::var("PRIME_API_KEY").unwrap_or_else(|_| {
            let path = required("ALLSTACK_HOSTED_PROOF_KEY_FILE");
            std::fs::read_to_string(path).expect("read private hosted proof key")
        });
        assert!(
            key.trim() == key && !key.contains(['\n', '\r']),
            "tenant API key must not contain surrounding whitespace"
        );
        Self {
            endpoint: endpoint.trim_end_matches('/').to_owned(),
            key,
            receipt: PathBuf::from(required("ALLSTACK_HOSTED_PROOF_RECEIPT")),
            http: Client::builder()
                .timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .no_proxy()
                .build()
                .expect("build bounded hosted client"),
        }
    }

    fn query_url(&self, entity_id: &str) -> Url {
        let mut url = Url::parse(&format!("{}/api/v1/events/query", self.endpoint)).unwrap();
        url.query_pairs_mut().extend_pairs([
            ("entity_id", entity_id),
            ("event_type_prefix", "prime."),
            ("order", "asc"),
            ("limit", "100"),
        ]);
        url
    }

    pub(super) async fn reject_auth(&self, entity_id: &str, writes: bool) {
        for key in [
            None,
            Some("ask_allstack_deliberately_invalid_hosted_sync_key"),
        ] {
            for method in [Method::GET, Method::POST] {
                if method == Method::POST && !writes {
                    continue;
                }
                let url = if method == Method::GET {
                    self.query_url(entity_id)
                } else {
                    Url::parse(&format!("{}/api/v1/events", self.endpoint)).unwrap()
                };
                let mut request = self.http.request(method.clone(), url);
                if method == Method::POST {
                    request = request.json(&json!({"event_type": EVENT, "entity_id": entity_id,
                        "payload": {"node_type": "decision", "properties": {"domain": "hosted-sync-proof"}}}));
                }
                if let Some(key) = key {
                    request = request.bearer_auth(key);
                }
                let response = request.send().await;
                assert!(response.is_ok(), "auth rejection request failed");
                assert!(
                    response.unwrap().status() == StatusCode::UNAUTHORIZED,
                    "missing/invalid key must reject GET and POST with 401"
                );
            }
        }
    }

    pub(super) async fn graph_node_matches(&self, proof: &Proof) {
        let mut url = Url::parse(&format!("{}/api/v1/prime/nodes/", self.endpoint)).unwrap();
        url.path_segments_mut()
            .expect("hosted node URL")
            .pop_if_empty()
            .push(&proof.entity_id);
        let response = self
            .http
            .get(url)
            .bearer_auth(&self.key)
            .send()
            .await
            .expect("hosted Prime exact-node read");
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "hosted Prime node missing"
        );
        let node: Value = response.json().await.expect("hosted Prime node JSON");
        assert_eq!(node["type"], "decision");
        assert_eq!(node["properties"], proof.expected_properties);
        let raw_id = proof.entity_id.rsplit(':').next().unwrap();
        assert!(
            node["id"] == raw_id || node["id"] == proof.entity_id,
            "hosted node ID mismatch"
        );
    }

    pub(super) async fn event_ids(&self, proof: &Proof) -> Vec<String> {
        // Gateway injects the authenticated tenant; never request an unfiltered scan.
        let response = self
            .http
            .get(self.query_url(&proof.entity_id))
            .bearer_auth(&self.key)
            .send()
            .await;
        assert!(response.is_ok(), "hosted exact-entity query failed");
        let response = response.unwrap();
        assert!(
            response.status() == StatusCode::OK,
            "hosted exact-entity query must return 200"
        );
        let body = response.json::<Value>().await;
        assert!(body.is_ok(), "hosted query must return JSON");
        let body = body.unwrap();
        let events = body["events"].as_array().expect("query events array");
        assert!(
            body["count"].as_u64() == Some(events.len() as u64),
            "query count mismatch"
        );
        assert!(
            body["total_count"].as_u64() == Some(events.len() as u64),
            "truncated proof query"
        );
        assert!(body["has_more"] == false, "proof query must be complete");
        assert!(
            events.len() <= 1,
            "duplicate or unexpected prime events for proof entity"
        );
        events
            .iter()
            .map(|event| {
                assert!(
                    event["entity_id"] == proof.entity_id,
                    "query returned another entity"
                );
                assert!(event["event_type"] == EVENT, "unexpected proof event type");
                assert!(
                    event["payload"]["node_type"] == "decision",
                    "wrong proof node type"
                );
                assert!(
                    event["payload"]["properties"] == proof.expected_properties,
                    "proof properties mismatch"
                );
                let id = event["id"].as_str().expect("remote event id");
                assert!(Uuid::parse_str(id).is_ok(), "remote event id must be UUID");
                id.to_owned()
            })
            .collect()
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Proof {
    pub(super) entity_id: String,
    pub(super) expected_properties: Value,
    pub(super) remote_event_ids: Vec<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    pub(super) schema: String,
    pub(super) endpoint: String,
    pub(super) binary_path: PathBuf,
    pub(super) cursor_before: DateTime<Utc>,
    pub(super) cursor_after: DateTime<Utc>,
    pub(super) proofs: Vec<Proof>,
}
