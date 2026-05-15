//! Ctrl-C / SIGTERM watcher returning a future that completes on signal.

use tokio::signal;
use tracing::info;

/// Resolves the first time the process receives a shutdown signal.
pub async fn watcher() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm = signal(SignalKind::terminate()).expect("install SIGTERM handler");
        let mut sigint = signal(SignalKind::interrupt()).expect("install SIGINT handler");
        tokio::select! {
            _ = sigterm.recv() => info!(target: "node::shutdown", "SIGTERM received"),
            _ = sigint.recv()  => info!(target: "node::shutdown", "SIGINT received"),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = signal::ctrl_c().await;
        info!(target: "node::shutdown", "Ctrl-C received");
    }
}
