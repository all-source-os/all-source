//! No adapter read transfers more events than it returns: every query names
//! an entity or a payload filter and a bounded page, and the expiry sweeps
//! read nothing at all.

use better_auth_allsource::AllsourceAuthAdapter;
use better_auth_core::{
    adapters::traits::{
        AccountOps, ApiKeyOps, InvitationOps, MemberOps, OrganizationOps, PasskeyOps, SessionOps,
        VerificationOps,
    },
    types::Account,
};
use chrono::{TimeZone, Utc};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};

const PAGE: usize = 200;

struct Recorded {
    url: String,
    reads: Arc<Mutex<Vec<String>>>,
}

impl Recorded {
    fn reads(&self) -> Vec<String> {
        self.reads.lock().unwrap().clone()
    }

    fn adapter(&self) -> AllsourceAuthAdapter {
        AllsourceAuthAdapter::new(&self.url, &self.url, "test-key")
    }
}

fn query_param(target: &str, key: &str) -> Option<String> {
    target
        .split_once('?')?
        .1
        .split('&')
        .find_map(|pair| pair.strip_prefix(&format!("{key}=")))
        .map(str::to_string)
}

/// Answers every GET with `respond(target)` and every POST with 200.
async fn serve_with(respond: impl Fn(&str) -> Vec<Value> + Send + Sync + 'static) -> Recorded {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let reads = Arc::new(Mutex::new(Vec::new()));
    let log = reads.clone();
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
            let mut parts = line.split_whitespace();
            let method = parts.next().unwrap_or("").to_string();
            let target = parts.next().unwrap_or("").to_string();
            let events = if method == "GET" {
                log.lock().unwrap().push(target.clone());
                respond(&target)
            } else {
                Vec::new()
            };
            let body = json!({"events": events, "count": events.len()}).to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            write_half.write_all(response.as_bytes()).await.unwrap();
            write_half.shutdown().await.ok();
        }
    });
    Recorded {
        url: format!("http://{addr}"),
        reads,
    }
}

fn assert_scoped(reads: &[String], op: &str) {
    for read in reads {
        let named = query_param(read, "entity_id").is_some()
            || query_param(read, "payload_filter").is_some();
        assert!(named, "{op}: unscoped read {read}");
        let limit: usize = query_param(read, "limit")
            .and_then(|l| l.parse().ok())
            .unwrap_or(usize::MAX);
        assert!(limit <= 1000, "{op}: read of {limit} events: {read}");
    }
}

#[tokio::test]
async fn every_by_field_read_is_filtered_by_core() {
    let recorded = serve_with(|_| Vec::new()).await;
    let adapter = recorded.adapter();

    let mut seen = 0;
    macro_rules! check {
        ($op:literal, $call:expr) => {{
            $call.await.unwrap();
            let reads = recorded.reads();
            assert!(reads.len() > seen, "{} made no read", $op);
            assert_scoped(&reads[seen..], $op);
            seen = reads.len();
        }};
    }
    check!("get_user_accounts", adapter.get_user_accounts("u1"));
    check!("get_user_sessions", adapter.get_user_sessions("u1"));
    check!("delete_user_sessions", adapter.delete_user_sessions("u1"));
    check!(
        "list_user_organizations",
        adapter.list_user_organizations("u1")
    );
    check!(
        "list_organization_members",
        adapter.list_organization_members("o1")
    );
    check!(
        "list_organization_invitations",
        adapter.list_organization_invitations("o1")
    );
    check!(
        "list_user_invitations",
        adapter.list_user_invitations("a@example.com")
    );
    check!("list_api_keys_by_user", adapter.list_api_keys_by_user("u1"));
    check!("list_passkeys_by_user", adapter.list_passkeys_by_user("u1"));
    let _ = seen;
}

#[tokio::test]
async fn the_expiry_sweeps_read_and_write_nothing() {
    let recorded = serve_with(|_| Vec::new()).await;
    let adapter = recorded.adapter();

    assert_eq!(adapter.delete_expired_sessions().await.unwrap(), 0);
    assert_eq!(adapter.delete_expired_verifications().await.unwrap(), 0);
    assert!(recorded.reads().is_empty(), "{:?}", recorded.reads());
}

fn account_event(n: usize, id: &str, user_id: &str) -> Value {
    let at =
        Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap() + chrono::Duration::minutes(n as i64);
    json!({
        "event_type": "auth.account.created",
        "entity_id": format!("auth-account:{id}"),
        "payload": serde_json::to_value(Account {
            id: id.into(),
            account_id: format!("acct-{id}"),
            provider_id: "google".into(),
            user_id: user_id.into(),
            access_token: None,
            refresh_token: None,
            id_token: None,
            access_token_expires_at: None,
            refresh_token_expires_at: None,
            scope: None,
            password: None,
            created_at: at,
            updated_at: at,
        })
        .unwrap(),
        "timestamp": at.to_rfc3339(),
        "version": 1,
    })
}

#[tokio::test]
async fn a_users_accounts_come_from_a_filtered_paged_read() {
    // 250 accounts for one user: a full first page, then a short second one.
    let recorded = serve_with(|target| {
        let filter = query_param(target, "payload_filter").unwrap_or_default();
        if !filter.contains("userId") {
            return Vec::new();
        }
        let offset: usize = query_param(target, "offset")
            .and_then(|o| o.parse().ok())
            .unwrap_or(0);
        (offset..(offset + PAGE).min(250))
            .map(|n| account_event(n, &format!("a{n}"), "u1"))
            .collect()
    })
    .await;

    let accounts = recorded.adapter().get_user_accounts("u1").await.unwrap();
    assert_eq!(accounts.len(), 250);
    assert!(accounts.iter().all(|a| a.user_id == "u1"));

    let reads: Vec<_> = recorded
        .reads()
        .into_iter()
        .filter(|r| {
            query_param(r, "payload_filter")
                .unwrap_or_default()
                .contains("userId")
        })
        .collect();
    assert_eq!(reads.len(), 2, "{reads:?}");
    assert_eq!(query_param(&reads[0], "offset").as_deref(), Some("0"));
    assert_eq!(query_param(&reads[1], "offset").as_deref(), Some("200"));
    assert_eq!(query_param(&reads[0], "limit").as_deref(), Some("200"));
}
