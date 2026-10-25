//! Sign-in lookups read one bounded, filtered window instead of every
//! verification or account the tenant ever created, honour tombstones, and an
//! unscoped 429 does not block the reads that follow it.

use better_auth_allsource::AllsourceAuthAdapter;
use better_auth_core::{
    adapters::traits::{AccountOps, VerificationOps},
    error::AuthError,
    types::{Account, CreateVerification, Verification},
};
use chrono::{DateTime, Duration as ChronoDuration, TimeZone, Utc};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};

const STATE: &str = "oauth:state-1";
const TAGGED: &str = "oauth:state-1#00000000-0000-0000-0000-000000000001";

/// One request the mock received.
#[derive(Clone)]
struct Request {
    method: String,
    target: String,
    body: String,
}

struct Recorded {
    url: String,
    requests: Arc<Mutex<Vec<Request>>>,
}

impl Recorded {
    fn requests(&self) -> Vec<Request> {
        self.requests.lock().unwrap().clone()
    }

    fn reads(&self) -> Vec<String> {
        self.requests()
            .into_iter()
            .filter(|r| r.method == "GET")
            .map(|r| r.target)
            .collect()
    }

    fn writes(&self) -> Vec<Value> {
        self.requests()
            .into_iter()
            .filter(|r| r.method == "POST")
            .map(|r| serde_json::from_str(&r.body).unwrap())
            .collect()
    }

    fn adapter(&self) -> AllsourceAuthAdapter {
        AllsourceAuthAdapter::new(&self.url, &self.url, "test-key")
    }
}

fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap();
                out.push(u8::from_str_radix(hex, 16).unwrap());
                i += 2;
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8(out).unwrap()
}

fn query_param(target: &str, key: &str) -> Option<String> {
    target
        .split_once('?')?
        .1
        .split('&')
        .find_map(|pair| pair.strip_prefix(&format!("{key}=")))
        .map(percent_decode)
}

fn http_response(status: &str, headers: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// Answers every GET with `respond(target)` and every POST with 200.
async fn serve_with(respond: impl Fn(&str) -> String + Send + Sync + 'static) -> Recorded {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let log = requests.clone();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let (read_half, mut write_half) = stream.into_split();
            let mut reader = BufReader::new(read_half);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            let mut length = 0;
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).await.unwrap();
                if header == "\r\n" || header.is_empty() {
                    break;
                }
                if let Some((name, value)) = header.split_once(':') {
                    if name.eq_ignore_ascii_case("content-length") {
                        length = value.trim().parse().unwrap();
                    }
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).await.unwrap();
            let mut parts = line.split_whitespace();
            let method = parts.next().unwrap_or("").to_string();
            let target = parts.next().unwrap_or("").to_string();
            log.lock().unwrap().push(Request {
                method: method.clone(),
                target: target.clone(),
                body: String::from_utf8(body).unwrap(),
            });
            let response = if method == "POST" {
                http_response("200 OK", "", "{}")
            } else {
                respond(&target)
            };
            write_half.write_all(response.as_bytes()).await.unwrap();
            write_half.shutdown().await.ok();
        }
    });
    Recorded {
        url: format!("http://{addr}"),
        requests,
    }
}

fn at(n: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap() + ChronoDuration::minutes(n)
}

fn event(n: i64, entity_id: &str, event_type: &str, payload: Value) -> Value {
    json!({
        "id": format!("00000000-0000-0000-0000-{n:012}"),
        "event_type": event_type,
        "entity_id": entity_id,
        "payload": payload,
        "timestamp": at(n).to_rfc3339(),
        "version": n,
    })
}

fn ok(list: Vec<Value>) -> String {
    let count = list.len();
    http_response(
        "200 OK",
        "",
        &json!({"events": list, "count": count}).to_string(),
    )
}

fn created(n: i64, id: &str, expires_at: DateTime<Utc>) -> Value {
    let payload = serde_json::to_value(Verification {
        id: id.into(),
        identifier: STATE.into(),
        value: "payload".into(),
        expires_at,
        created_at: at(0),
        updated_at: at(0),
    })
    .unwrap();
    event(
        n,
        &format!("auth-verification:{id}"),
        "auth.verification.created",
        payload,
    )
}

fn live() -> DateTime<Utc> {
    Utc::now() + ChronoDuration::minutes(10)
}

async fn lookup(recorded: &Recorded) -> Option<Verification> {
    recorded
        .adapter()
        .get_verification_by_identifier(STATE)
        .await
        .unwrap()
}

#[tokio::test]
async fn verification_lookup_is_one_filtered_read() {
    let recorded = serve_with(|_| ok(vec![created(1, TAGGED, live())])).await;

    let got = lookup(&recorded).await.expect("live verification");
    assert_eq!(got.id, TAGGED);

    let reads = recorded.reads();
    assert_eq!(reads.len(), 1, "{reads:?}");
    assert_eq!(
        query_param(&reads[0], "event_type_prefix").as_deref(),
        Some("auth.verification.")
    );
    let filter: Value = serde_json::from_str(&query_param(&reads[0], "payload_filter").unwrap())
        .expect("payload_filter is JSON");
    assert_eq!(filter, json!({"identifier": STATE}));
    assert_eq!(query_param(&reads[0], "limit").as_deref(), Some("16"));
}

#[tokio::test]
async fn a_consumed_verification_is_hidden_by_its_tombstone_in_the_same_read() {
    let tombstone = event(
        2,
        &format!("auth-verification:{TAGGED}"),
        "auth.verification.deleted",
        json!({"_deleted": true, "id": TAGGED, "identifier": STATE}),
    );
    let recorded =
        serve_with(move |_| ok(vec![tombstone.clone(), created(1, TAGGED, live())])).await;

    assert!(
        lookup(&recorded).await.is_none(),
        "a used sign-in state must not be accepted again"
    );
    assert_eq!(recorded.reads().len(), 1);
}

#[tokio::test]
async fn a_verification_issued_before_tagged_ids_is_checked_against_its_entity() {
    let legacy = "00000000-0000-0000-0000-0000000000aa";
    let consumed = event(
        2,
        &format!("auth-verification:{legacy}"),
        "auth.verification.consumed",
        json!({"_deleted": true, "id": format!("auth-verification:{legacy}")}),
    );
    let recorded = serve_with(move |target| {
        if query_param(target, "entity_id").is_some() {
            ok(vec![consumed.clone(), created(1, legacy, live())])
        } else {
            ok(vec![created(1, legacy, live())])
        }
    })
    .await;

    assert!(lookup(&recorded).await.is_none());
    let reads = recorded.reads();
    assert_eq!(reads.len(), 2, "{reads:?}");
    assert_eq!(
        query_param(&reads[1], "entity_id"),
        Some(format!("auth-verification:{legacy}"))
    );
}

#[tokio::test]
async fn an_expired_verification_does_not_resolve() {
    let expired = Utc::now() - ChronoDuration::minutes(1);
    let recorded = serve_with(move |_| ok(vec![created(1, TAGGED, expired)])).await;
    assert!(lookup(&recorded).await.is_none());
}

#[tokio::test]
async fn created_ids_name_their_identifier_and_tombstones_carry_it() {
    let recorded = serve_with(|_| ok(vec![])).await;
    let adapter = recorded.adapter();

    let v = adapter
        .create_verification(CreateVerification {
            identifier: STATE.into(),
            value: "payload".into(),
            expires_at: live(),
        })
        .await
        .unwrap();
    assert!(v.id.starts_with("oauth:state-1#"), "{}", v.id);
    adapter.delete_verification(&v.id).await.unwrap();

    let writes = recorded.writes();
    assert_eq!(writes.len(), 2);
    assert_eq!(writes[1]["event_type"], "auth.verification.deleted");
    assert_eq!(
        writes[1]["payload"],
        json!({"_deleted": true, "id": v.id, "identifier": STATE})
    );
}

#[tokio::test]
async fn an_identifier_too_long_for_a_tagged_id_keeps_a_bare_uuid() {
    let recorded = serve_with(|_| ok(vec![])).await;
    let v = recorded
        .adapter()
        .create_verification(CreateVerification {
            identifier: format!("email-verification:{}@example.com", "a".repeat(100)),
            value: "payload".into(),
            expires_at: live(),
        })
        .await
        .unwrap();
    assert!(!v.id.contains('#'), "{}", v.id);
    let entity = recorded.writes()[0]["entity_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(entity.len() <= 128, "{entity}");
}

fn account(n: i64, token: &str) -> Value {
    serde_json::to_value(Account {
        id: "a1".into(),
        account_id: "google-123".into(),
        provider_id: "google".into(),
        user_id: "u1".into(),
        access_token: Some(token.into()),
        refresh_token: None,
        id_token: None,
        access_token_expires_at: None,
        refresh_token_expires_at: None,
        scope: None,
        password: None,
        created_at: at(0),
        updated_at: at(n),
    })
    .unwrap()
}

#[tokio::test]
async fn account_lookup_is_one_filtered_read_of_the_current_state() {
    let recorded = serve_with(|_| {
        ok(vec![
            event(
                2,
                "auth-account:a1",
                "auth.account.updated",
                account(2, "new"),
            ),
            event(
                1,
                "auth-account:a1",
                "auth.account.created",
                account(1, "old"),
            ),
        ])
    })
    .await;

    let got = recorded
        .adapter()
        .get_account("google", "google-123")
        .await
        .unwrap()
        .expect("account");
    assert_eq!(got.access_token.as_deref(), Some("new"));

    let reads = recorded.reads();
    assert_eq!(reads.len(), 1, "{reads:?}");
    assert_eq!(
        query_param(&reads[0], "event_type_prefix").as_deref(),
        Some("auth.account.")
    );
    let filter: Value =
        serde_json::from_str(&query_param(&reads[0], "payload_filter").unwrap()).unwrap();
    assert_eq!(
        filter,
        json!({"providerId": "google", "accountId": "google-123"})
    );
}

#[tokio::test]
async fn a_deleted_account_is_hidden_and_its_tombstone_names_the_pair() {
    let tombstone =
        json!({"_deleted": true, "id": "a1", "providerId": "google", "accountId": "google-123"});
    let deleted = tombstone.clone();
    let recorded = serve_with(move |target| {
        let created = event(
            1,
            "auth-account:a1",
            "auth.account.created",
            account(1, "t"),
        );
        if query_param(target, "entity_id").is_some() {
            return ok(vec![created]);
        }
        ok(vec![
            event(
                2,
                "auth-account:a1",
                "auth.account.deleted",
                deleted.clone(),
            ),
            created,
        ])
    })
    .await;
    let adapter = recorded.adapter();

    assert!(adapter
        .get_account("google", "google-123")
        .await
        .unwrap()
        .is_none());

    adapter.delete_account("a1").await.unwrap();
    assert_eq!(recorded.writes()[0]["payload"], tombstone);
}

#[tokio::test]
async fn an_unscoped_429_refuses_one_read_without_blocking_the_next() {
    let recorded = serve_with(|_| {
        http_response(
            "429 Too Many Requests",
            "Retry-After: 30\r\n",
            r#"{"error":"egress_budget_unavailable","reason":"busy"}"#,
        )
    })
    .await;
    let adapter = recorded.adapter();

    for _ in 0..3 {
        let err = adapter
            .get_verification_by_identifier(STATE)
            .await
            .unwrap_err();
        assert!(matches!(err, AuthError::RateLimited), "{err:?}");
    }
    assert_eq!(
        recorded.reads().len(),
        3,
        "each read reaches the server; a full pool is not a tenant refusal"
    );
}
