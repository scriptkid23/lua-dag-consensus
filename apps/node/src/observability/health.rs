//! Axum-based health + metrics endpoints.

use std::{net::SocketAddr, str::FromStr, sync::Arc};

use axum::{
    Router,
    extract::State,
    response::{IntoResponse, Response},
    routing::get,
};
use tokio::net::TcpListener;
use tracing::info;

use super::metrics::Metrics;

/// Shared HTTP state.
#[derive(Clone)]
struct AdminState {
    metrics: Arc<Metrics>,
}

/// Start the admin HTTP server (`/healthz`, `/readyz`, `/metrics`).
///
/// Returns once the listener has bound; the spawned task runs until
/// `shutdown_signal` resolves.
pub async fn serve_admin(
    addr: &str,
    metrics: Arc<Metrics>,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    let socket: SocketAddr = SocketAddr::from_str(addr)?;
    let state = AdminState { metrics };
    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics_handler))
        .with_state(state);
    let listener = TcpListener::bind(socket).await?;
    info!(target: "node::admin", "admin listening on {addr}");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(shutdown)
            .await;
    });
    Ok(())
}

async fn healthz() -> &'static str {
    "ok"
}

async fn readyz() -> &'static str {
    "ready"
}

async fn metrics_handler(State(s): State<AdminState>) -> Response {
    match s.metrics.render() {
        Ok(text) => text.into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
