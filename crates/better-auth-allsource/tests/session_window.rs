//! The bounded session read resolves the same session as a full-history
//! read, over recorded entity histories and against a Core that does or does
//! not honour `order`, and a rate-limit refusal fails fast.

use better_auth_allsource::AllsourceAuthAdapter;
use better_auth_core::{adapters::traits::SessionOps, error::AuthError, types::Session};
use chrono::{DateTime, Duration as ChronoDuration, TimeZone, Utc};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};

const TOKEN: &str = "session_abc";

#[derive(Clone, Copy)]
enum Core {
    HonoursOrder,
    IgnoresOrder,
}

struct Recorded {
    url: String,
    requests: Arc<Mutex<Vec<String>>>,
}

fn query_param<'a>(request_target: &'a str, key: &str) -> Option<&'a str> {
    request_target
        .split_once('?')?
        .1
        .split('&')
        .find_map(|pair| pair.strip_prefix(&format!("{key}=")))
}

fn http_response(status: &str, headers: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// Serves `history` (oldest first) for every query: newest-first when `core`
/// honours `order=desc`, oldest-first otherwise, truncated to `limit`.
async fn serve(history: Vec<Value>, core: Core) -> Recorded {
    serve_with(move |target| {
        let limit = query_param(target, "limit")
            .and_then(|l| l.parse::<usize>().ok())
            .unwrap_or(usize::MAX);
        let desc = query_param(target, "order") == Some("desc");
        let mut events = history.clone();
        if desc && matches!(core, Core::HonoursOrder) {
            events.reverse();
        }
        events.truncate(limit);
        let count = events.len();
        http_response(
            "200 OK",
            "",
            &json!({"events": events, "count": count}).to_string(),
        )
    })
    .await
}

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
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).await.unwrap();
                if header == "\r\n" || header.is_empty() {
                    break;
                }
            }
            let target = line.split_whitespace().nth(1).unwrap_or("").to_string();
            log.lock().unwrap().push(target.clone());
            write_half
                .write_all(respond(&target).as_bytes())
                .await
                .unwrap();
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

fn session_payload(n: i64) -> Value {
    let session = Session {
        id: "sess-1".into(),
        token: TOKEN.into(),
        user_id: "user-1".into(),
        expires_at: at(1_000 + n),
        created_at: at(0),
        updated_at: at(n),
        ip_address: None,
        user_agent: None,
        impersonated_by: None,
        active_organization_id: Some(format!("org-{n}")),
        active: true,
    };
    serde_json::to_value(session).unwrap()
}

fn event(n: i64, event_type: &str, payload: Value) -> Value {
    json!({
        "id": format!("00000000-0000-0000-0000-{n:012}"),
        "event_type": event_type,
        "entity_id": format!("auth-session:{TOKEN}"),
        "payload": payload,
        "timestamp": at(n).to_rfc3339(),
        "version": n,
    })
}

/// A session created, then updated `updates` times, optionally tombstoned.
fn history(updates: i64, tombstoned: bool) -> Vec<Value> {
    let mut events = vec![event(0, "auth.session.created", session_payload(0))];
    for n in 1..=updates {
        events.push(event(n, "auth.session.updated", session_payload(n)));
    }
    if tombstoned {
        events.push(event(
            updates + 1,
            "auth.session.deleted",
            json!({"_deleted": true, "id": format!("auth-session:{TOKEN}")}),
        ));
    }
    events
}

/// The full-history answer: the newest event by (timestamp, version), or
/// `None` when it is a tombstone.
fn expected(history: &[Value]) -> Option<String> {
    let newest = history.iter().max_by_key(|e| {
        (
            e["timestamp"].as_str().unwrap().to_string(),
            e["version"].as_i64(),
        )
    })?;
    if newest["payload"]["_deleted"] == json!(true) {
        return None;
    }
    newest["payload"]["activeOrganizationId"]
        .as_str()
        .map(str::to_string)
}

fn limits(recorded: &Recorded) -> Vec<String> {
    recorded
        .requests
        .lock()
        .unwrap()
        .iter()
        .map(|t| query_param(t, "limit").unwrap_or("").to_string())
        .collect()
}

async fn read_session(recorded: &Recorded) -> Option<Session> {
    AllsourceAuthAdapter::new(&recorded.url, &recorded.url, "test-key")
        .get_session(TOKEN)
        .await
        .unwrap()
}

#[tokio::test]
async fn bounded_read_matches_the_full_history_over_recorded_entities() {
    let cases = [
        (history(0, false), Core::HonoursOrder, vec!["16"]),
        (history(4, false), Core::HonoursOrder, vec!["16"]),
        (history(4, false), Core::IgnoresOrder, vec!["16"]),
        (history(4, true), Core::HonoursOrder, vec!["16"]),
        (history(4, true), Core::IgnoresOrder, vec!["16"]),
        (history(13, true), Core::IgnoresOrder, vec!["16"]),
        (history(14, true), Core::IgnoresOrder, vec!["16", "1000"]),
        (history(15, false), Core::IgnoresOrder, vec!["16", "1000"]),
        (history(40, false), Core::HonoursOrder, vec!["16", "1000"]),
        (history(40, true), Core::HonoursOrder, vec!["16", "1000"]),
    ];
    for (i, (events, core, want_limits)) in cases.into_iter().enumerate() {
        let want = expected(&events);
        let recorded = serve(events, core).await;
        let got = read_session(&recorded).await;
        assert_eq!(
            got.as_ref().and_then(|s| s.active_organization_id.clone()),
            want,
            "case {i}"
        );
        if let Some(session) = got {
            assert!(session.active, "case {i}");
        }
        assert_eq!(limits(&recorded), want_limits, "case {i}");
    }
}

#[tokio::test]
async fn a_core_that_ignores_order_takes_the_overflow_path_and_stays_correct() {
    let events = history(30, false);
    let want = expected(&events);
    assert_eq!(want.as_deref(), Some("org-30"));
    let recorded = serve(events, Core::IgnoresOrder).await;

    let got = read_session(&recorded).await.expect("live session");
    assert_eq!(got.active_organization_id, want);
    assert_eq!(limits(&recorded), ["16", "1000"]);
    let first = recorded.requests.lock().unwrap()[0].clone();
    assert_eq!(query_param(&first, "order"), Some("desc"));
}

#[tokio::test]
async fn a_newest_tombstone_beyond_the_window_still_hides_the_session() {
    let events = history(30, true);
    assert_eq!(expected(&events), None);
    let recorded = serve(events, Core::IgnoresOrder).await;
    assert!(read_session(&recorded).await.is_none());
    assert_eq!(limits(&recorded), ["16", "1000"]);
}

#[tokio::test]
async fn a_rate_limit_is_a_typed_refusal_that_fails_fast() {
    let recorded = serve_with(|_| {
        http_response(
            "429 Too Many Requests",
            "Retry-After: 3\r\n",
            r#"{"error":"tenant_rate_limited","scope":"tenant","retry_after":3}"#,
        )
    })
    .await;
    let adapter = AllsourceAuthAdapter::new(&recorded.url, &recorded.url, "test-key");

    for _ in 0..5 {
        let err = adapter.get_session(TOKEN).await.unwrap_err();
        assert!(matches!(err, AuthError::RateLimited), "{err:?}");
    }
    assert_eq!(
        recorded.requests.lock().unwrap().len(),
        1,
        "calls inside Retry-After make no request"
    );
}

#[tokio::test]
async fn a_bare_503_stays_a_database_error() {
    let recorded =
        serve_with(|_| http_response("503 Service Unavailable", "", r#"{"error":"down"}"#)).await;
    let adapter = AllsourceAuthAdapter::new(&recorded.url, &recorded.url, "test-key");
    let err = adapter.get_session(TOKEN).await.unwrap_err();
    assert!(matches!(err, AuthError::Database(_)), "{err:?}");
    assert!(err.to_string().contains("down"), "{err}");
}
