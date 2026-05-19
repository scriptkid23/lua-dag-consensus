//! Axum-based health + metrics endpoints.

use std::{net::SocketAddr, str::FromStr, sync::Arc};

use axum::{
    Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tracing::info;

use super::metrics::Metrics;

/// Shared HTTP state.
#[derive(Clone)]
pub(crate) struct AdminState {
    metrics: Arc<Metrics>,
    /// `true` once the live gossip swarm has bound its listen addrs.
    /// `/readyz` waits on this; `/healthz` ignores it.
    net_ready: watch::Receiver<bool>,
}

/// Start the admin HTTP server (`/healthz`, `/readyz`, `/metrics`).
///
/// Returns once the listener has bound; the spawned task runs until
/// `shutdown_signal` resolves.
///
/// `/readyz` flips to `200 OK` only after `net_ready` is set — this gates
/// orchestration tools that wait for a live swarm. `/healthz` reports
/// process-liveness only and is always `200 OK`.
pub async fn serve_admin(
    addr: &str,
    metrics: Arc<Metrics>,
    net_ready: watch::Receiver<bool>,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    let socket: SocketAddr = SocketAddr::from_str(addr)?;
    let state = AdminState { metrics, net_ready };
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

async fn readyz(State(s): State<AdminState>) -> Response {
    if *s.net_ready.borrow() {
        (StatusCode::OK, "ready").into_response()
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "warming").into_response()
    }
}

async fn metrics_handler(State(s): State<AdminState>) -> Response {
    match s.metrics.render() {
        Ok(text) => text.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn readyz_returns_503_until_watch_flips() {
        let (tx, rx) = watch::channel(false);
        let m = Arc::new(Metrics::new().unwrap());
        let state = AdminState {
            metrics: m,
            net_ready: rx,
        };
        // Direct unit check — avoids spinning up a real server.
        let resp_pre = readyz(State(state.clone())).await;
        assert_eq!(resp_pre.status(), StatusCode::SERVICE_UNAVAILABLE);

        tx.send(true).unwrap();
        let resp_post = readyz(State(state)).await;
        assert_eq!(resp_post.status(), StatusCode::OK);
    }
}
