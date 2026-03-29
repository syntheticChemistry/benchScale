// SPDX-License-Identifier: AGPL-3.0-only
//! JSON-RPC 2.0 server for benchScale (UniBin compliance).
//!
//! Implements newline-delimited JSON-RPC over TCP per
//! `PRIMAL_IPC_PROTOCOL.md` v3.1. Exposes `health.*`, `lab.*`,
//! `topology.*`, and `node.*` method families.

pub mod methods;

use std::net::SocketAddr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info, warn};

use methods::ServerState;

/// Standard JSON-RPC 2.0 error codes.
mod error_codes {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL_ERROR: i64 = -32603;
}

/// JSON-RPC 2.0 request envelope.
#[derive(Debug, Deserialize)]
struct RpcRequest {
    jsonrpc: String,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
    id: serde_json::Value,
}

/// JSON-RPC 2.0 success response.
#[derive(Debug, Serialize)]
struct RpcResponse {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
    id: serde_json::Value,
}

/// JSON-RPC 2.0 error object.
#[derive(Debug, Serialize)]
struct RpcError {
    code: i64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

impl RpcResponse {
    fn success(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0",
            result: Some(result),
            error: None,
            id,
        }
    }

    fn error(id: serde_json::Value, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
                data: None,
            }),
            id,
        }
    }
}

/// Run the benchScale JSON-RPC server.
///
/// Binds TCP on `addr`, accepts connections, and dispatches
/// newline-delimited JSON-RPC 2.0 requests to method handlers.
/// Runs in `--standalone` mode by default (no Songbird registration).
pub async fn run_server(addr: SocketAddr, standalone: bool) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    info!("benchScale JSON-RPC server listening on {addr}");

    if standalone {
        info!("running in standalone mode (no Songbird registration)");
    } else if let Ok(family_id) = std::env::var("FAMILY_ID") {
        info!("FAMILY_ID={family_id} — Songbird registration not yet implemented");
    } else {
        warn!("FAMILY_ID not set and not standalone — degrading to standalone mode");
    }

    let state = Arc::new(ServerState::new().await?);

    loop {
        let (stream, peer) = listener.accept().await?;
        info!("accepted connection from {peer}");
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, &state).await {
                error!("connection error from {peer}: {e}");
            }
        });
    }
}

async fn handle_connection(stream: TcpStream, state: &ServerState) -> anyhow::Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await? {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let response = dispatch(&line, state).await;

        let mut out = serde_json::to_vec(&response)?;
        out.push(b'\n');
        writer.write_all(&out).await?;
    }

    Ok(())
}

async fn dispatch(line: &str, state: &ServerState) -> RpcResponse {
    let request: RpcRequest = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => {
            return RpcResponse::error(
                serde_json::Value::Null,
                error_codes::PARSE_ERROR,
                format!("Parse error: {e}"),
            );
        }
    };

    if request.jsonrpc != "2.0" {
        return RpcResponse::error(
            request.id,
            error_codes::INVALID_REQUEST,
            "Invalid JSON-RPC version (must be \"2.0\")",
        );
    }

    let id = request.id.clone();

    match methods::dispatch(&request.method, request.params, state).await {
        Ok(result) => RpcResponse::success(id, result),
        Err(MethodError::NotFound) => {
            RpcResponse::error(id, error_codes::METHOD_NOT_FOUND, format!("Method not found: {}", request.method))
        }
        Err(MethodError::InvalidParams(msg)) => {
            RpcResponse::error(id, error_codes::INVALID_PARAMS, msg)
        }
        Err(MethodError::Internal(msg)) => {
            RpcResponse::error(id, error_codes::INTERNAL_ERROR, msg)
        }
    }
}

/// Method dispatch error kinds.
#[derive(Debug)]
pub enum MethodError {
    /// Method name not recognised.
    NotFound,
    /// Parameters failed validation.
    InvalidParams(String),
    /// Server-side failure.
    Internal(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpc_response_success_serialization() {
        let resp = RpcResponse::success(
            serde_json::Value::Number(1.into()),
            serde_json::json!({"status": "ok"}),
        );
        let json = serde_json::to_string(&resp).expect("serialize");
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"result\""));
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn test_rpc_response_error_serialization() {
        let resp = RpcResponse::error(
            serde_json::Value::Number(1.into()),
            error_codes::METHOD_NOT_FOUND,
            "Method not found",
        );
        let json = serde_json::to_string(&resp).expect("serialize");
        assert!(json.contains("\"error\""));
        assert!(json.contains("-32601"));
        assert!(!json.contains("\"result\""));
    }

    #[test]
    fn test_parse_valid_request() {
        let line = r#"{"jsonrpc":"2.0","method":"health.liveness","params":{},"id":1}"#;
        let req: RpcRequest = serde_json::from_str(line).expect("parse");
        assert_eq!(req.method, "health.liveness");
        assert_eq!(req.jsonrpc, "2.0");
    }

    #[test]
    fn test_parse_invalid_json() {
        let line = "not json at all";
        assert!(serde_json::from_str::<RpcRequest>(line).is_err());
    }
}
