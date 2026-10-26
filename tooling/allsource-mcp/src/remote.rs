//! A store read over HTTP from the `AllSource` gateway, and the seam the view
//! tools read through.
//!
//! Views are folded by the Query Service and stored in Core, so only a gateway
//! can answer them; an embedded data directory has no view rows to read.

use std::{future::Future, time::Duration};

use anyhow::{Context, Result};
use serde_json::{Value, json};

use crate::stores::Store;

/// Upper bound on a request that does not long-poll.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// One gateway answer, whatever its status; [`gateway_reply`] classifies it.
#[derive(Debug)]
pub struct HttpReply {
    pub status: u16,
    pub etag: Option<String>,
    pub body: Value,
}

/// Reads of the latest-state view routes.
pub trait ViewReader {
    /// `GET /api/v1/views/{view}/entities/{key}`.
    fn latest_state(
        &self,
        view: &str,
        key: &str,
        if_none_match: Option<&str>,
    ) -> impl Future<Output = Result<HttpReply>>;

    /// `GET /api/v1/views/{view}/entities?prefix&after&limit`.
    fn latest_page(
        &self,
        view: &str,
        prefix: Option<&str>,
        after: Option<&str>,
        limit: usize,
    ) -> impl Future<Output = Result<HttpReply>>;

    /// `GET /api/v1/views/{view}/changes?after&limit&wait`.
    fn changes(
        &self,
        view: &str,
        after: u64,
        limit: usize,
        wait_seconds: u64,
    ) -> impl Future<Output = Result<HttpReply>>;
}

pub struct RemoteStore {
    base: reqwest::Url,
    api_key: Option<String>,
    client: reqwest::Client,
}

impl RemoteStore {
    /// Bind a gateway base URL and the key every request carries.
    ///
    /// Credentials in the URL are refused: the URL is echoed in `list_stores`
    /// and result context, the key is not.
    pub fn new(url: &str, api_key: Option<String>) -> Result<Self> {
        let base = reqwest::Url::parse(url).with_context(|| format!("--remote '{url}'"))?;
        if !matches!(base.scheme(), "http" | "https") || base.cannot_be_a_base() {
            anyhow::bail!("--remote must be an http(s) URL, got '{url}'");
        }
        if !base.username().is_empty() || base.password().is_some() {
            anyhow::bail!("--remote must not carry credentials; set ALLSOURCE_API_KEY instead");
        }
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .user_agent(concat!("allsource-mcp/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            base,
            api_key: api_key
                .map(|key| key.trim().to_string())
                .filter(|key| !key.is_empty()),
            client,
        })
    }

    /// The gateway URL, safe to report.
    pub fn display_url(&self) -> String {
        self.base.to_string()
    }

    async fn get(
        &self,
        segments: &[&str],
        query: &[(&str, String)],
        if_none_match: Option<&str>,
        timeout: Duration,
    ) -> Result<HttpReply> {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .map_err(|()| anyhow::anyhow!("--remote URL cannot carry a path"))?
            .pop_if_empty()
            .extend(segments);
        if !query.is_empty() {
            let mut pairs = url.query_pairs_mut();
            for (name, value) in query {
                pairs.append_pair(name, value);
            }
        }

        let mut request = self.client.get(url).timeout(timeout);
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }
        if let Some(tag) = if_none_match {
            request = request.header(reqwest::header::IF_NONE_MATCH, tag);
        }
        let response = request
            .send()
            .await
            .map_err(|error| anyhow::anyhow!("gateway request failed: {error}"))?;
        let status = response.status().as_u16();
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let text = response
            .text()
            .await
            .map_err(|error| anyhow::anyhow!("gateway response unreadable: {error}"))?;
        let body = if text.trim().is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).unwrap_or_else(|_| json!({ "raw": text }))
        };
        Ok(HttpReply { status, etag, body })
    }
}

impl ViewReader for RemoteStore {
    async fn latest_state(
        &self,
        view: &str,
        key: &str,
        if_none_match: Option<&str>,
    ) -> Result<HttpReply> {
        self.get(
            &["api", "v1", "views", view, "entities", key],
            &[],
            if_none_match,
            REQUEST_TIMEOUT,
        )
        .await
    }

    async fn latest_page(
        &self,
        view: &str,
        prefix: Option<&str>,
        after: Option<&str>,
        limit: usize,
    ) -> Result<HttpReply> {
        let mut query = vec![("limit", limit.to_string())];
        if let Some(prefix) = prefix {
            query.push(("prefix", prefix.to_string()));
        }
        if let Some(after) = after {
            query.push(("after", after.to_string()));
        }
        self.get(
            &["api", "v1", "views", view, "entities"],
            &query,
            None,
            REQUEST_TIMEOUT,
        )
        .await
    }

    async fn changes(
        &self,
        view: &str,
        after: u64,
        limit: usize,
        wait_seconds: u64,
    ) -> Result<HttpReply> {
        let query = [
            ("after", after.to_string()),
            ("limit", limit.to_string()),
            ("wait", wait_seconds.to_string()),
        ];
        // Core holds the request for up to `wait`, so the deadline grows with it.
        self.get(
            &["api", "v1", "views", view, "changes"],
            &query,
            None,
            REQUEST_TIMEOUT + Duration::from_secs(wait_seconds),
        )
        .await
    }
}

/// The refusal an embedded store gives every view read.
fn needs_remote() -> anyhow::Error {
    anyhow::anyhow!(
        "unsupported: views are read from the gateway; start the server with --remote <url> and select that store"
    )
}

impl ViewReader for Store {
    fn latest_state(
        &self,
        _view: &str,
        _key: &str,
        _if_none_match: Option<&str>,
    ) -> impl Future<Output = Result<HttpReply>> {
        std::future::ready(Err(needs_remote()))
    }

    fn latest_page(
        &self,
        _view: &str,
        _prefix: Option<&str>,
        _after: Option<&str>,
        _limit: usize,
    ) -> impl Future<Output = Result<HttpReply>> {
        std::future::ready(Err(needs_remote()))
    }

    fn changes(
        &self,
        _view: &str,
        _after: u64,
        _limit: usize,
        _wait_seconds: u64,
    ) -> impl Future<Output = Result<HttpReply>> {
        std::future::ready(Err(needs_remote()))
    }
}

/// Pass through 200, 304 and the statuses a route gives meaning to; turn every
/// other status into the error prefix `execute_tool` classifies.
pub fn gateway_reply(reply: HttpReply, meaningful: &[u16]) -> Result<HttpReply> {
    if matches!(reply.status, 200 | 304) || meaningful.contains(&reply.status) {
        return Ok(reply);
    }
    let detail = reply
        .body
        .get("detail")
        .or_else(|| reply.body.get("error"))
        .and_then(Value::as_str)
        .unwrap_or("");
    match reply.status {
        400 => anyhow::bail!("invalid argument: gateway rejected the request: {detail}"),
        401 | 403 => anyhow::bail!(
            "access denied: gateway refused this key ({}): {detail}",
            reply.status
        ),
        404 => anyhow::bail!("not found: {detail}"),
        status => anyhow::bail!("gateway answered {status}: {detail}"),
    }
}

#[cfg(test)]
pub mod mock {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Answer exactly one HTTP request with a canned response; the handle
    /// yields the raw request head. `headers` is zero or more `name: value\r\n`.
    pub async fn serve_once(
        status: &'static str,
        headers: &'static str,
        body: &'static str,
    ) -> (String, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock gateway");
        let base = format!("http://{}", listener.local_addr().expect("mock address"));
        let handle = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut request = Vec::new();
            let mut buf = [0u8; 1024];
            loop {
                let read = socket.read(&mut buf).await.expect("read request");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buf[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            let response = format!(
                "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n{headers}\r\n{body}",
                body.len()
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("write response");
            socket.shutdown().await.ok();
            String::from_utf8_lossy(&request).to_string()
        });
        (base, handle)
    }
}

#[cfg(test)]
mod tests {
    use super::{HttpReply, RemoteStore, ViewReader, gateway_reply, mock::serve_once};
    use serde_json::{Value, json};

    #[tokio::test]
    async fn a_latest_read_carries_the_key_and_tag_and_returns_the_etag() {
        let (base, request) = serve_once(
            "200 OK",
            "etag: \"sha256:new\"\r\ncontent-type: application/json\r\n",
            r#"{"view":"ops_status","key":"a/b","state":{"ok":1},"version":3}"#,
        )
        .await;
        let remote = RemoteStore::new(&base, Some(" key-1 ".to_string())).expect("remote");

        let reply = remote
            .latest_state("ops_status", "a/b", Some("\"sha256:old\""))
            .await
            .expect("reply");
        let head = request.await.expect("mock").to_ascii_lowercase();

        assert_eq!(reply.status, 200);
        assert_eq!(reply.etag.as_deref(), Some("\"sha256:new\""));
        assert_eq!(reply.body["state"]["ok"], 1);
        assert!(
            head.starts_with("get /api/v1/views/ops_status/entities/a%2fb http/1.1"),
            "a '/' in a key stays inside one path segment: {head}"
        );
        assert!(head.contains("authorization: bearer key-1"), "{head}");
        assert!(head.contains("if-none-match: \"sha256:old\""), "{head}");
    }

    #[tokio::test]
    async fn a_base_path_is_kept_and_page_arguments_become_the_query() {
        let (base, request) = serve_once("200 OK", "", r#"{"rows":[],"next":null}"#).await;
        let remote = RemoteStore::new(&format!("{base}/gw/"), None).expect("remote");

        remote
            .latest_page("ops_status", Some("probe-a"), Some("probe-a:1"), 2)
            .await
            .expect("reply");
        let head = request.await.expect("mock");

        assert!(
            head.starts_with(
                "GET /gw/api/v1/views/ops_status/entities?limit=2&prefix=probe-a&after=probe-a%3A1 HTTP/1.1"
            ),
            "{head}"
        );
        assert!(
            !head.to_ascii_lowercase().contains("authorization:"),
            "no key configured, no header sent: {head}"
        );
    }

    #[tokio::test]
    async fn a_not_modified_answer_has_no_body() {
        let (base, _request) =
            serve_once("304 Not Modified", "etag: \"sha256:same\"\r\n", "").await;
        let remote = RemoteStore::new(&base, None).expect("remote");

        let reply = remote
            .latest_state("ops_status", "k", Some("\"sha256:same\""))
            .await
            .expect("reply");

        assert_eq!(reply.status, 304);
        assert_eq!(reply.body, Value::Null);
        assert!(gateway_reply(reply, &[]).is_ok());
    }

    #[test]
    fn a_remote_url_must_be_http_and_carry_no_credentials() {
        assert!(RemoteStore::new("ftp://gateway", None).is_err());
        assert!(RemoteStore::new("not a url", None).is_err());
        assert!(RemoteStore::new("https://user:secret@gateway", None).is_err());
        assert!(RemoteStore::new("https://gateway", None).is_ok());
    }

    #[test]
    fn gateway_statuses_map_to_the_error_classes_tools_report() {
        let reply = |status: u16| HttpReply {
            status,
            etag: None,
            body: json!({ "error": "nope" }),
        };
        let message = |status: u16| {
            gateway_reply(reply(status), &[])
                .expect_err("refused")
                .to_string()
        };

        assert!(message(400).starts_with("invalid argument:"));
        assert!(message(401).starts_with("access denied:"));
        assert!(message(403).starts_with("access denied:"));
        assert!(message(404).starts_with("not found:"));
        assert!(message(503).starts_with("gateway answered 503"));
        assert!(
            gateway_reply(reply(409), &[409]).is_ok(),
            "a route may claim a status"
        );
    }
}
