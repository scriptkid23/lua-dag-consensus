//! Top-level binary wiring.

use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use consensus::StateMachine;
use net::Bridge;
use storage::{Database, RocksPersistence};
use tokio::sync::{mpsc, watch};
use tracing::info;

use crate::{
    args::Args,
    config::NodeConfig,
    observability::{health, metrics::Metrics, tracing as tracing_init},
    orchestrator::Orchestrator,
    rpc_server, shutdown,
    timer::TokioClock,
};

/// Synchronous entry point used by `main.rs`.
pub fn run() -> Result<()> {
    let args = Args::parse();
    tracing_init::init();
    let cfg = NodeConfig::load(&args)?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move { run_async(cfg).await })
}

async fn run_async(cfg: NodeConfig) -> Result<()> {
    info!(target: "node", "starting LUA-DAG node");

    // Storage.
    let db = Arc::new(Database::open(&cfg.storage)?);
    let persistence = RocksPersistence::new(db);

    // Consensus.
    let sm = StateMachine::new(cfg.consensus.clone());
    let _clock = TokioClock::new();

    // Bridge.
    let (events_tx, events_rx) = mpsc::channel(1024);
    let (bridge, _bridge_handle) = Bridge::with_channels(events_tx, 1024);

    // Observability.
    let metrics = Arc::new(Metrics::new()?);

    // Graceful shutdown plumbing.
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let admin_shutdown = subscribe_to_shutdown(shutdown_rx.clone());
    let rpc_shutdown = subscribe_to_shutdown(shutdown_rx.clone());

    // HTTP surfaces.
    health::serve_admin(&cfg.admin_listen, metrics.clone(), admin_shutdown).await?;
    rpc_server::serve(&cfg.rpc_listen, rpc_shutdown).await?;

    // Orchestrator.
    let orch = Orchestrator::new(sm, bridge, events_rx, persistence, metrics);
    let orch_task = tokio::spawn(orch.run());

    // Wait for signal.
    shutdown::watcher().await;
    info!(target: "node", "shutdown signal received — draining");
    let _ = shutdown_tx.send(true);

    let _ = orch_task.await;
    Ok(())
}

async fn subscribe_to_shutdown(mut rx: watch::Receiver<bool>) {
    while !*rx.borrow() {
        if rx.changed().await.is_err() {
            break;
        }
    }
}
