//! JSON-RPC server placeholder. Real method registration arrives with
//! `consensus::api::ConsensusQuery` wiring.

use std::{net::SocketAddr, str::FromStr};

use axum::{Json, Router, routing::post};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tracing::info;

/// JSON-RPC 2.0 request envelope.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct RpcReq {
    /// JSON-RPC version (must be `"2.0"`).
    pub jsonrpc: String,
    /// Method name.
    pub method: String,
    /// Opaque params.
    #[serde(default)]
    pub params: serde_json::Value,
    /// Request id.
    pub id: serde_json::Value,
}

/// JSON-RPC 2.0 response envelope.
#[derive(Debug, Serialize)]
pub struct RpcResp {
    /// Always `"2.0"`.
    pub jsonrpc: &'static str,
    /// Echoed request id.
    pub id: serde_json::Value,
    /// Result (skeleton always returns `null`).
    pub result: serde_json::Value,
}

async fn rpc(Json(req): Json<RpcReq>) -> Json<RpcResp> {
    info!(target: "node::rpc", method = %req.method, "rpc method invoked");
    // Skeleton: every method returns null.
    Json(RpcResp {
        jsonrpc: "2.0",
        id: req.id,
        result: serde_json::Value::Null,
    })
}

/// Start the JSON-RPC HTTP server.
pub async fn serve(
    addr: &str,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    let socket: SocketAddr = SocketAddr::from_str(addr)?;
    let app = Router::new().route("/", post(rpc));
    let listener = TcpListener::bind(socket).await?;
    info!(target: "node::rpc", "rpc listening on {addr}");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(shutdown)
            .await;
    });
    Ok(())
}
