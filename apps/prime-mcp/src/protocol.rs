//! MCP JSON-RPC protocol types.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC request.
#[derive(Debug, Deserialize)]
pub struct Request {
    #[allow(dead_code)]
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

/// JSON-RPC response.
#[derive(Debug, Serialize)]
pub struct Response {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

impl Response {
    pub fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Option<Value>, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(RpcError { code, message }),
        }
    }
}

/// Server capabilities for the `initialize` response. `instructions` reaches
/// the client's model before it plans any tool call.
pub fn server_info(_auto_inject: bool, instructions: Option<String>) -> Value {
    let capabilities = serde_json::json!({ "tools": {}, "resources": {} });
    let mut info = serde_json::json!({
        "protocolVersion": "2024-11-05",
        "capabilities": capabilities,
        "serverInfo": {
            "name": "allsource-prime",
            "version": env!("CARGO_PKG_VERSION")
        }
    });
    if let Some(instructions) = instructions {
        info["instructions"] = Value::String(instructions);
    }
    info
}
