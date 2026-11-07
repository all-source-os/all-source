//! A credential provider is asked for a bearer before every request; the
//! static-key constructor keeps sending its key.

use better_auth_allsource::AllsourceAuthAdapter;
use better_auth_core::adapters::traits::ApiKeyOps;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};

/// Answers every request with an empty page and records its Authorization header.
async fn recording_server() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let (read_half, mut write_half) = stream.into_split();
            let mut reader = BufReader::new(read_half);
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).await.unwrap();
                if header == "\r\n" || header.is_empty() {
                    break;
                }
                if let Some(value) = header.to_ascii_lowercase().strip_prefix("authorization: ") {
                    log.lock().unwrap().push(value.trim().to_string());
                }
            }
            let body = r#"{"events":[],"count":0}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            write_half.write_all(response.as_bytes()).await.unwrap();
        }
    });
    (url, seen)
}

#[tokio::test]
async fn every_request_asks_the_provider_for_a_fresh_bearer() {
    let (url, seen) = recording_server().await;
    let minted = Arc::new(AtomicUsize::new(0));
    let counter = minted.clone();
    let adapter = AllsourceAuthAdapter::with_credential_provider(&url, &url, move || {
        Ok(format!("token-{}", counter.fetch_add(1, Ordering::SeqCst)))
    });

    adapter.get_api_key_by_id("a").await.unwrap();
    adapter.get_api_key_by_id("b").await.unwrap();

    assert_eq!(
        *seen.lock().unwrap(),
        vec!["bearer token-0", "bearer token-1"]
    );
}

#[tokio::test]
async fn the_static_key_constructor_is_unchanged() {
    let (url, seen) = recording_server().await;
    let adapter = AllsourceAuthAdapter::new(&url, &url, "test-key");

    adapter.get_api_key_by_id("a").await.unwrap();

    assert_eq!(*seen.lock().unwrap(), vec!["bearer test-key"]);
}

#[tokio::test]
async fn a_provider_failure_sends_nothing() {
    let (url, seen) = recording_server().await;
    let adapter = AllsourceAuthAdapter::with_credential_provider(&url, &url, || {
        Err("signing key unreadable".to_string())
    });

    let err = adapter.get_api_key_by_id("a").await.unwrap_err();

    assert!(err.to_string().contains("signing key unreadable"), "{err}");
    assert!(seen.lock().unwrap().is_empty());
}
