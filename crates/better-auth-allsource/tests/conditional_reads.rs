//! Every read names its caller, an unchanged answer is revalidated with
//! `If-None-Match` and costs no body, and with a read TTL a fresh answer is
//! served without a request until a write through the adapter clears it.

use better_auth_allsource::AllsourceAuthAdapter;
use better_auth_core::{
    adapters::traits::{SessionOps, UserOps},
    types::Session,
};
use chrono::{TimeZone, Utc};
use serde_json::{json, Value};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};

const TOKEN: &str = "session_abc";

#[derive(Debug, Clone)]
struct Request {
    method: String,
    caller: Option<String>,
    if_none_match: Option<String>,
    status: u16,
}

struct Store {
    url: String,
    requests: Arc<Mutex<Vec<Request>>>,
}

impl Store {
    fn requests(&self) -> Vec<Request> {
        self.requests.lock().unwrap().clone()
    }

    fn reads(&self) -> Vec<Request> {
        self.requests()
            .into_iter()
            .filter(|r| r.method == "GET")
            .collect()
    }
}

fn session_event(version: i64) -> Value {
    let at = Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap();
    let session = Session {
        id: "sess-1".into(),
        token: TOKEN.into(),
        user_id: "user-1".into(),
        expires_at: at + chrono::Duration::days(1),
        created_at: at,
        updated_at: at,
        ip_address: None,
        user_agent: None,
        impersonated_by: None,
        active_organization_id: Some(format!("org-{version}")),
        active: true,
    };
    json!({
        "event_type": "auth.session.created",
        "entity_id": format!("auth-session:{TOKEN}"),
        "payload": session,
        "timestamp": (at + chrono::Duration::minutes(version)).to_rfc3339(),
        "version": version,
    })
}

/// A Core that holds one entity's history, tags each answer with the history
/// length, answers `304` to a matching `If-None-Match`, and appends whatever
/// payload is POSTed.
async fn serve(history: Vec<Value>) -> Store {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let log = requests.clone();
    let history = Arc::new(Mutex::new(history));
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let (read_half, mut write_half) = stream.into_split();
            let mut reader = BufReader::new(read_half);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            let method = line.split_whitespace().next().unwrap_or("").to_string();
            let (mut caller, mut if_none_match, mut length) = (None, None, 0usize);
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).await.unwrap();
                if header == "\r\n" || header.is_empty() {
                    break;
                }
                let (name, value) = header.split_once(':').unwrap_or((&header, ""));
                let value = value.trim().to_string();
                match name.to_ascii_lowercase().as_str() {
                    "x-allsource-caller" => caller = Some(value),
                    "if-none-match" => if_none_match = Some(value),
                    "content-length" => length = value.parse().unwrap_or(0),
                    _ => {}
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).await.unwrap();

            let (status, response) = {
                let mut history = history.lock().unwrap();
                if method == "POST" {
                    let posted: Value = serde_json::from_slice(&body).unwrap();
                    let version = history.len() as i64;
                    let mut event = session_event(version);
                    event["payload"] = posted["payload"].clone();
                    history.push(event);
                    (
                        200,
                        "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}"
                            .to_string(),
                    )
                } else {
                    let etag = format!("\"v{}\"", history.len());
                    if if_none_match.as_deref() == Some(etag.as_str()) {
                        (304, format!("HTTP/1.1 304 Not Modified\r\nETag: {etag}\r\nConnection: close\r\n\r\n"))
                    } else {
                        let mut events = history.clone();
                        events.reverse();
                        let body = json!({"events": events, "count": events.len()}).to_string();
                        (
                            200,
                            format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nETag: {etag}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                                body.len()
                            ),
                        )
                    }
                }
            };
            log.lock().unwrap().push(Request {
                method,
                caller,
                if_none_match,
                status,
            });
            write_half.write_all(response.as_bytes()).await.unwrap();
            write_half.shutdown().await.ok();
        }
    });
    Store {
        url: format!("http://{addr}"),
        requests,
    }
}

fn org(session: Option<Session>) -> Option<String> {
    session.and_then(|s| s.active_organization_id)
}

#[tokio::test]
async fn every_read_names_its_caller_by_auth_entity() {
    let store = serve(vec![session_event(0)]).await;
    let adapter = AllsourceAuthAdapter::new(&store.url, &store.url, "test-key");

    adapter.get_session(TOKEN).await.unwrap();
    // The store serves session events to every query, so user reads fail to
    // decode; only the headers they sent matter here.
    let _ = adapter.get_user_by_id("user-1").await;
    let _ = adapter.get_user_by_email("a@b.io").await;

    let callers: Vec<_> = store.reads().into_iter().map(|r| r.caller).collect();
    assert_eq!(callers[0].as_deref(), Some("better_auth.session"));
    assert_eq!(callers[1].as_deref(), Some("better_auth.user"));
    assert!(
        callers[2..]
            .iter()
            .all(|c| c.as_deref() == Some("better_auth.user")),
        "{callers:?}"
    );
}

#[tokio::test]
async fn a_caller_prefix_replaces_better_auth() {
    let store = serve(vec![session_event(0)]).await;
    let adapter =
        AllsourceAuthAdapter::new(&store.url, &store.url, "test-key").with_caller_prefix("lh.auth");

    adapter.get_session(TOKEN).await.unwrap();

    assert_eq!(store.reads()[0].caller.as_deref(), Some("lh.auth.session"));
}

#[tokio::test]
async fn an_unchanged_answer_is_revalidated_and_costs_no_body() {
    let store = serve(vec![session_event(0)]).await;
    let adapter = AllsourceAuthAdapter::new(&store.url, &store.url, "test-key");

    let first = org(adapter.get_session(TOKEN).await.unwrap());
    let second = org(adapter.get_session(TOKEN).await.unwrap());

    assert_eq!(first.as_deref(), Some("org-0"));
    assert_eq!(second, first);
    let reads = store.reads();
    assert_eq!(reads.len(), 2);
    assert_eq!(reads[0].if_none_match, None);
    assert_eq!(reads[1].if_none_match.as_deref(), Some("\"v1\""));
    assert_eq!(reads[1].status, 304);
}

#[tokio::test]
async fn a_changed_answer_replaces_the_remembered_one() {
    let store = serve(vec![session_event(0)]).await;
    let adapter = AllsourceAuthAdapter::new(&store.url, &store.url, "test-key");
    let other = AllsourceAuthAdapter::new(&store.url, &store.url, "test-key");

    assert!(adapter.get_session(TOKEN).await.unwrap().is_some());
    other.delete_session(TOKEN).await.unwrap();

    assert!(adapter.get_session(TOKEN).await.unwrap().is_none());
    let reads = store.reads();
    assert_eq!(reads[1].status, 200, "the old tag no longer matches");
}

#[tokio::test]
async fn a_read_ttl_serves_fresh_answers_without_a_request() {
    let store = serve(vec![session_event(0)]).await;
    let adapter = AllsourceAuthAdapter::new(&store.url, &store.url, "test-key")
        .with_read_ttl(Duration::from_secs(60));

    for _ in 0..5 {
        assert_eq!(
            org(adapter.get_session(TOKEN).await.unwrap()).as_deref(),
            Some("org-0")
        );
    }

    assert_eq!(store.reads().len(), 1);
}

#[tokio::test]
async fn a_write_through_the_adapter_clears_its_read_ttl() {
    let store = serve(vec![session_event(0)]).await;
    let adapter = AllsourceAuthAdapter::new(&store.url, &store.url, "test-key")
        .with_read_ttl(Duration::from_secs(60));

    assert!(adapter.get_session(TOKEN).await.unwrap().is_some());
    adapter.delete_session(TOKEN).await.unwrap();

    assert!(adapter.get_session(TOKEN).await.unwrap().is_none());
    assert_eq!(store.reads().len(), 2);
}

#[tokio::test]
async fn an_expired_read_ttl_revalidates() {
    let store = serve(vec![session_event(0)]).await;
    let adapter = AllsourceAuthAdapter::new(&store.url, &store.url, "test-key")
        .with_read_ttl(Duration::from_millis(50));

    adapter.get_session(TOKEN).await.unwrap();
    tokio::time::sleep(Duration::from_millis(80)).await;
    adapter.get_session(TOKEN).await.unwrap();

    let reads = store.reads();
    assert_eq!(reads.len(), 2);
    assert_eq!(reads[1].status, 304);
}
