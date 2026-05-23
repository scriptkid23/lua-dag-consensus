//! JSON-RPC server with L3 read-only query methods (plan 06b-l3).

use std::{net::SocketAddr, str::FromStr, sync::Arc};

use axum::{Json, Router, routing::post};
use borsh::to_vec;
use consensus::api::tier::BlobStatus;
use consensus::api::query::ConsensusQuery;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tracing::{info, warn};
use types::primitives::{BlobId, Height};

use crate::query::RocksConsensusQuery;

/// JSON-RPC 2.0 request envelope.
#[derive(Debug, Deserialize)]
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
    /// Result payload.
    pub result: serde_json::Value,
}

async fn rpc(
    axum::extract::State(query): axum::extract::State<Arc<RocksConsensusQuery>>,
    Json(req): Json<RpcReq>,
) -> Json<RpcResp> {
    info!(target: "node::rpc", method = %req.method, "rpc method invoked");
    let result = match req.method.as_str() {
        "lua_getLatestFinalized" => latest_finalized(&query),
        "lua_getMacroCheckpointAt" => macro_checkpoint_at(&query, &req.params),
        "lua_getBlobStatus" => blob_status_at(&query, &req.params),
        _ => serde_json::Value::Null,
    };
    Json(RpcResp {
        jsonrpc: "2.0",
        id: req.id,
        result,
    })
}

fn latest_finalized(query: &RocksConsensusQuery) -> serde_json::Value {
    match query.latest_finalized() {
        Ok(Some(qc)) => {
            let mode = format!("{:?}", qc.mode);
            serde_json::json!({
                "checkpoint_hash": hex::encode(qc.checkpoint_hash.0),
                "mode": mode,
            })
        }
        Ok(None) => serde_json::Value::Null,
        Err(e) => {
            warn!(target: "node::rpc", error = %e, "latest_finalized query failed");
            serde_json::Value::Null
        }
    }
}

fn macro_checkpoint_at(query: &RocksConsensusQuery, params: &serde_json::Value) -> serde_json::Value {
    let Some(height_raw) = params.get(0).and_then(|v| v.as_u64()) else {
        return serde_json::Value::Null;
    };
    let height = Height(height_raw);
    match query.macro_checkpoint_at(height) {
        Ok(Some(cp)) => match to_vec(&cp) {
            Ok(bytes) => serde_json::json!({
                "height": height_raw,
                "checkpoint_borsh_hex": hex::encode(bytes),
            }),
            Err(e) => {
                warn!(target: "node::rpc", error = %e, "macro checkpoint encode failed");
                serde_json::Value::Null
            }
        },
        Ok(None) => serde_json::Value::Null,
        Err(e) => {
            warn!(target: "node::rpc", error = %e, "macro_checkpoint_at query failed");
            serde_json::Value::Null
        }
    }
}

fn blob_status_wire_name(status: BlobStatus) -> &'static str {
    match status {
        BlobStatus::Accepted => "accepted",
        BlobStatus::SoftConfirmed => "soft_confirmed",
        BlobStatus::Justified => "justified",
        BlobStatus::Finalized => "finalized",
        BlobStatus::EpochFinalized => "epoch_finalized",
    }
}

fn blob_status_at(query: &RocksConsensusQuery, params: &serde_json::Value) -> serde_json::Value {
    let Some(hex_raw) = params.get(0).and_then(|v| v.as_str()) else {
        return serde_json::Value::Null;
    };
    let hex_str = hex_raw.strip_prefix("0x").unwrap_or(hex_raw);
    let Ok(bytes) = hex::decode(hex_str) else {
        return serde_json::Value::Null;
    };
    if bytes.len() != 32 {
        return serde_json::Value::Null;
    }
    let mut id = [0u8; 32];
    id.copy_from_slice(&bytes);
    let blob = BlobId(id);
    match query.blob_status(&blob) {
        Ok(status) => serde_json::json!({
            "blob_id": format!("0x{}", hex::encode(id)),
            "status": blob_status_wire_name(status),
        }),
        Err(e) => {
            warn!(target: "node::rpc", error = %e, "blob_status query failed");
            serde_json::Value::Null
        }
    }
}

/// Start the JSON-RPC HTTP server.
pub async fn serve(
    addr: &str,
    query: Arc<RocksConsensusQuery>,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    let socket: SocketAddr = SocketAddr::from_str(addr)?;
    let app = Router::new()
        .route("/", post(rpc))
        .with_state(query);
    let listener = TcpListener::bind(socket).await?;
    info!(target: "node::rpc", "rpc listening on {addr}");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(shutdown)
            .await;
    });
    Ok(())
}
