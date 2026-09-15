//! MCP stdio transport: newline-delimited JSON-RPC.
//!
//! **One response per line, terminated by `\n`, with no `Content-Length` header.**
//! The MCP stdio transport is newline-delimited — Content-Length framing belongs
//! to the Language Server Protocol, and a client reading line by line cannot
//! parse it: the header, the blank line and a body with no trailing newline each
//! fail to be a JSON-RPC message, and the next response's header lands on the
//! same line as the previous body. The visible symptom is
//! `MCP server connection timed out`, because `initialize` is answered in
//! milliseconds and the client never manages to read the answer.
//!
//! That is #179, and it reached this crate a second time (#285) because the
//! original fix corrected prime's copy and this one kept the same shape. The
//! invariant is pinned by a test here now, not by a doc comment.
//!
//! Input stays tolerant: `read_message` accepts either framing, so a client that
//! still sends Content-Length keeps working.

use allsource_core::embedded::EmbeddedCore;
use anyhow::Result;
use std::io::{BufRead, Write};

use crate::{
    diagnostics::DiagnosticPolicy,
    protocol::{self, Request, Response},
    tools,
};

pub struct StdioTransport {
    core: EmbeddedCore,
    policy: DiagnosticPolicy,
}

impl StdioTransport {
    /// Create a transport bound to one core and diagnostic policy.
    pub fn new(core: EmbeddedCore, policy: DiagnosticPolicy) -> Self {
        Self { core, policy }
    }

    /// Serve MCP requests until standard input closes.
    pub async fn run(&mut self) -> Result<()> {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
        let mut reader = std::io::BufReader::new(stdin.lock());
        self.serve(&mut reader, &mut stdout).await?;

        // No `shutdown()`: the core is opened read-only, and `EmbeddedCore::shutdown`
        // syncs the WAL and flushes storage with no read-only guard — a write path
        // beside whichever process owns the data dir (#201).
        tracing::info!("stdin closed");
        Ok(())
    }

    /// The request loop, over any reader and writer, so it can be driven in a test.
    pub async fn serve(&mut self, reader: &mut impl BufRead, writer: &mut impl Write) -> Result<()> {
        loop {
            let Some(body) = read_message(reader)? else {
                break; // EOF
            };

            let body = body.trim().to_string();
            if body.is_empty() {
                continue;
            }

            tracing::debug!("recv: {body}");

            let request: Request = match serde_json::from_str(&body) {
                Ok(r) => r,
                Err(e) => {
                    let resp = Response::error(None, -32700, format!("Parse error: {e}"));
                    write_response(writer, &resp)?;
                    continue;
                }
            };

            let response = self.handle_request(&request).await;

            if let Some(resp) = response {
                write_response(writer, &resp)?;
            }
        }
        Ok(())
    }

    /// Route one JSON-RPC request, returning no response for notifications.
    async fn handle_request(&self, req: &Request) -> Option<Response> {
        match req.method.as_str() {
            "initialize" => {
                let requested = req
                    .params
                    .as_ref()
                    .and_then(|params| params.get("protocolVersion"))
                    .and_then(serde_json::Value::as_str);
                let negotiated = if requested == Some(protocol::CURRENT_PROTOCOL_VERSION) {
                    protocol::CURRENT_PROTOCOL_VERSION
                } else {
                    protocol::LEGACY_PROTOCOL_VERSION
                };
                Some(Response::success(
                    req.id.clone(),
                    protocol::server_info(negotiated),
                ))
            }

            // Notification — no response
            "notifications/initialized" => None,

            "tools/list" => {
                let defs = tools::tool_definitions(&self.policy);
                Some(Response::success(
                    req.id.clone(),
                    serde_json::json!({ "tools": defs }),
                ))
            }

            "tools/call" => {
                let params = req.params.as_ref();
                let tool_name = params
                    .and_then(|p| p.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let args = params
                    .and_then(|p| p.get("arguments"))
                    .cloned()
                    .unwrap_or(serde_json::json!({}));

                let result = tools::execute_tool(&self.core, &self.policy, tool_name, &args).await;
                Some(Response::success(req.id.clone(), result))
            }

            // Ignore other notifications silently
            method if method.starts_with("notifications/") => None,

            _ => Some(Response::error(
                req.id.clone(),
                -32601,
                format!("Method not found: {}", req.method),
            )),
        }
    }
}

/// Read a single MCP message from the reader.
///
/// Supports two modes:
/// 1. **Content-Length framing** (MCP spec): headers ending with blank line, then exact body bytes
/// 2. **Line-delimited fallback**: if first line starts with `{`, treat as line-delimited JSON
fn read_message(reader: &mut impl BufRead) -> Result<Option<String>> {
    let mut first_line = String::new();
    let bytes_read = reader.read_line(&mut first_line)?;
    if bytes_read == 0 {
        return Ok(None); // EOF
    }

    let trimmed = first_line.trim();

    // Fallback: if the line starts with `{`, it's line-delimited JSON (backward compat)
    if trimmed.starts_with('{') {
        return Ok(Some(trimmed.to_string()));
    }

    // Content-Length framing: parse headers
    let content_length = if let Some(value) = trimmed.strip_prefix("Content-Length:") {
        value
            .trim()
            .parse::<usize>()
            .map_err(|e| anyhow::anyhow!("invalid Content-Length: {e}"))?
    } else {
        // Unknown header line — skip until we find Content-Length or empty line
        return read_message(reader); // recurse to find next message
    };

    // Read remaining headers until blank line
    loop {
        let mut header = String::new();
        let n = reader.read_line(&mut header)?;
        if n == 0 {
            return Ok(None); // EOF
        }
        if header.trim().is_empty() {
            break; // End of headers
        }
        // Ignore other headers (Content-Type, etc.)
    }

    // Read exact body
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body)?;

    Ok(Some(String::from_utf8_lossy(&body).to_string()))
}

#[cfg(test)]
mod tests {
    use allsource_core::embedded::{Config, EmbeddedCore};

    use super::StdioTransport;
    use crate::diagnostics::{AccessProfile, DiagnosticPolicy};

    async fn transport() -> StdioTransport {
        let core = EmbeddedCore::open(Config::builder().build().expect("valid config"))
            .await
            .expect("in-memory core");
        let policy =
            DiagnosticPolicy::new(AccessProfile::Local, None, "local").expect("local policy");
        StdioTransport::new(core, policy)
    }

    async fn serve(input: &str) -> String {
        let mut transport = transport().await;
        let mut reader = std::io::BufReader::new(input.as_bytes());
        let mut out: Vec<u8> = Vec::new();
        transport
            .serve(&mut reader, &mut out)
            .await
            .expect("serve runs to EOF");
        String::from_utf8(out).expect("responses are utf-8")
    }

    /// #179, and #285 when the same shape survived into this crate: Content-Length
    /// framing makes every response unparseable to a newline-delimited client, and
    /// the only symptom is a connection timeout.
    #[tokio::test]
    async fn every_response_is_one_newline_terminated_line_without_a_header() {
        let out = serve(concat!(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            "\n",
        ))
        .await;

        assert!(
            !out.contains("Content-Length"),
            "stdio transport must not emit LSP framing: {out}"
        );
        assert!(
            out.ends_with('\n'),
            "the last response must be terminated, or it concatenates with the next"
        );

        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2, "one line per response, got {lines:?}");
        for line in lines {
            let parsed: serde_json::Value =
                serde_json::from_str(line).expect("each line parses on its own");
            assert_eq!(parsed["jsonrpc"], "2.0");
        }
    }

    /// Input stays tolerant of the framing this server no longer writes.
    #[tokio::test]
    async fn a_content_length_framed_request_is_still_accepted() {
        let body = r#"{"jsonrpc":"2.0","id":7,"method":"tools/list"}"#;
        let out = serve(&format!(
            "Content-Length: {}\r\n\r\n{body}",
            body.len()
        ))
        .await;

        let parsed: serde_json::Value =
            serde_json::from_str(out.trim_end()).expect("one JSON line back");
        assert_eq!(parsed["id"], 7);
    }

    /// A notification has no id and must draw no response at all — a reply to one
    /// is an unmatched message that desynchronizes the client.
    #[tokio::test]
    async fn a_notification_produces_no_output() {
        let out = serve("{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n").await;
        assert!(out.is_empty(), "expected silence, got {out:?}");
    }
}

/// Write one JSON-RPC response as a single newline-terminated line.
///
/// `serde_json::to_string` is compact and contains no newline, so the `writeln!`
/// terminator is the only one in the output and the line stays parseable.
fn write_response(writer: &mut impl Write, response: &Response) -> Result<()> {
    let json = serde_json::to_string(response)?;
    tracing::debug!("send: {json}");
    writeln!(writer, "{json}")?;
    writer.flush()?;
    Ok(())
}
