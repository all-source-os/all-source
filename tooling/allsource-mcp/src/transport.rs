//! MCP stdio transport with Content-Length header framing (per MCP spec).
//!
//! Reads JSON-RPC messages from stdin using `Content-Length: N\r\n\r\n<body>` framing
//! (identical to the Language Server Protocol). Falls back to line-delimited JSON
//! if the first line looks like JSON (for backward compatibility with simple tests).

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

    /// Serve framed MCP requests until standard input closes.
    pub async fn run(&mut self) -> Result<()> {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
        let mut reader = std::io::BufReader::new(stdin.lock());

        loop {
            let Some(body) = read_message(&mut reader)? else {
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
                    write_response(&mut stdout, &resp)?;
                    continue;
                }
            };

            let response = self.handle_request(&request).await;

            if let Some(resp) = response {
                write_response(&mut stdout, &resp)?;
            }
        }

        tracing::info!("stdin closed, shutting down");
        self.core.shutdown().await?;
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

/// Write one Content-Length-framed JSON-RPC response.
fn write_response(stdout: &mut impl Write, response: &Response) -> Result<()> {
    let json = serde_json::to_string(response)?;
    tracing::debug!("send: {json}");
    write!(stdout, "Content-Length: {}\r\n\r\n{}", json.len(), json)?;
    stdout.flush()?;
    Ok(())
}
