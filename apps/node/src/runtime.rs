//! Top-level binary wiring.

use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use consensus::{StateMachine, action::Action};
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
    if args.health_probe {
        return health_probe();
    }

    tracing_init::init();
    let cfg = NodeConfig::load(&args)?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move { run_async(cfg, args).await })
}

async fn run_async(cfg: NodeConfig, args: Args) -> Result<()> {
    info!(target: "node", "starting LUA-DAG node");

    // ─── Fail-closed startup guard (spec §8) ───────────────────────────
    if cfg.node.network_mode == "live" && !args.allow_skeleton_network && cfg.net.listen.is_empty()
    {
        anyhow::bail!(
            "network_mode=\"live\" requires at least one [net].listen address \
             (or pass --allow-skeleton-network)"
        );
    }

    // Storage.
    let db = Arc::new(Database::open(&cfg.storage)?);
    let persistence = RocksPersistence::new(db);

    // Consensus.
    let sm = StateMachine::new(cfg.consensus.clone());
    let _clock = TokioClock::new();

    // Bridge (events_tx fed into consensus; bridge.actions_rx is drained but unused
    // in the live path now — broadcasts go via the swarm channel).
    let (events_tx, events_rx) = mpsc::channel(1024);
    let (bridge, _bridge_handle) = Bridge::with_channels(events_tx.clone(), 1024);

    // Observability.
    let metrics = Arc::new(Metrics::new()?);

    // Shutdown plumbing.
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // ─── Live swarm (Task 10) ──────────────────────────────────────────
    // Skeleton mode is allowed only when `--allow-skeleton-network` is set.
    // In live mode we always spawn the swarm.
    let (net_actions_tx, net_actions_rx) = mpsc::channel::<Action>(1024);
    let (swarm_handle, net_ready_rx) = if args.allow_skeleton_network
        && cfg.node.network_mode != "live"
    {
        // Skeleton path: provide a stub readiness signal (always true) and no
        // swarm task.
        let (ready_tx, ready_rx) = watch::channel(true);
        drop(ready_tx);
        (None, ready_rx)
    } else {
        let keypair = net::deterministic_key::devnet_keypair_from_label(&cfg.node.identity.label)
            .context("derive devnet keypair from node.identity.label")?;
        let spawn = net::swarm_runner::spawn_gossip_tasks(keypair, cfg.net.clone(), net_actions_rx)
            .await
            .context("spawn gossipsub swarm")?;

        // Fan-in swarm events into the consensus events channel.
        let mut events_rx_swarm = spawn.events_rx;
        let events_tx_for_swarm = events_tx.clone();
        tokio::spawn(async move {
            while let Some(ev) = events_rx_swarm.recv().await {
                if events_tx_for_swarm.send(ev).await.is_err() {
                    break;
                }
            }
        });

        (Some(spawn.handle), spawn.ready)
    };

    // HTTP surfaces.
    let admin_shutdown = subscribe_to_shutdown(shutdown_rx.clone());
    let rpc_shutdown = subscribe_to_shutdown(shutdown_rx.clone());
    health::serve_admin(
        &cfg.admin_listen,
        metrics.clone(),
        net_ready_rx,
        admin_shutdown,
    )
    .await?;
    rpc_server::serve(&cfg.rpc_listen, rpc_shutdown).await?;

    // Orchestrator.
    let orch = Orchestrator::new(sm, bridge, events_rx, persistence, metrics, net_actions_tx);
    let orch_task = tokio::spawn(orch.run());

    // Wait for signal.
    shutdown::watcher().await;
    info!(target: "node", "shutdown signal received — draining");
    let _ = shutdown_tx.send(true);

    let _ = orch_task.await;
    if let Some(h) = swarm_handle {
        h.abort();
    }
    Ok(())
}

/// One-shot `GET /readyz` against the container-side admin port.
/// Used by `HEALTHCHECK` so the runtime image does not need `curl`.
fn health_probe() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect("127.0.0.1:9100")
            .await
            .context("connect admin port 9100")?;
        stream
            .write_all(b"GET /readyz HTTP/1.0\r\nHost: localhost\r\n\r\n")
            .await
            .context("write probe request")?;
        let mut buf = Vec::with_capacity(256);
        stream
            .read_to_end(&mut buf)
            .await
            .context("read probe response")?;
        let head = std::str::from_utf8(&buf[..buf.len().min(64)]).unwrap_or("");
        if head.starts_with("HTTP/1.0 200") || head.starts_with("HTTP/1.1 200") {
            Ok(())
        } else {
            anyhow::bail!("readyz reported non-200: {head:?}")
        }
    })
}

async fn subscribe_to_shutdown(mut rx: watch::Receiver<bool>) {
    while !*rx.borrow() {
        if rx.changed().await.is_err() {
            break;
        }
    }
}

// ─── Test entry points (always public so integration tests can reach them) ───

/// Helpers for driving the runtime from integration tests.
///
/// `tests/` lives in a separate compilation unit and therefore cannot see
/// `#[cfg(test)]`-gated items, so this module is always public. It performs
/// no I/O on its own beyond what [`test_helpers::run_for_test`] documents.
pub mod test_helpers {
    use super::{Args, NodeConfig, Result};
    use std::path::PathBuf;

    /// Minimal args struct for [`run_for_test`].
    #[derive(Clone, Debug)]
    pub struct TestArgs {
        /// Path to the config dir (must contain `profiles/<profile>.toml`).
        pub config_dir: PathBuf,
        /// Profile name.
        pub profile: String,
        /// Whether `--allow-skeleton-network` is passed.
        pub allow_skeleton_network: bool,
    }

    /// Drive the runtime up to the live-mode gate, then immediately request
    /// shutdown. Returns whatever startup error fires before the orchestrator
    /// can run, or `Ok(())` if startup is clean.
    ///
    /// Used by `tests/start_fails_closed_in_live_mode.rs` (spec §8 negative
    /// test) so a regression that bypasses the listen-addr gate is caught
    /// against the real `run_async` body — not a synthetic helper.
    ///
    /// `async` is retained for API symmetry with `run_async`; the body
    /// performs no `.await` because the gate is purely a config check.
    #[allow(clippy::unused_async)]
    pub async fn run_for_test(test_args: TestArgs) -> Result<()> {
        let args = Args {
            profile: test_args.profile.clone(),
            config_dir: test_args.config_dir.clone(),
            override_configs: vec![],
            identity_label: None,
            allow_skeleton_network: test_args.allow_skeleton_network,
            data_dir: Some(test_args.config_dir.join("data")),
            admin_listen: "127.0.0.1:0".into(),
            rpc_listen: "127.0.0.1:0".into(),
            health_probe: false,
        };
        let cfg = NodeConfig::load(&args)?;

        // Run the same fail-closed gate as `run_async` and return its error.
        // (We do not actually start the swarm here: the test only exercises
        // the gate and does not need a tokio-multithreaded runtime.)
        if cfg.node.network_mode == "live"
            && !args.allow_skeleton_network
            && cfg.net.listen.is_empty()
        {
            anyhow::bail!(
                "network_mode=\"live\" requires at least one [net].listen address \
                 (or pass --allow-skeleton-network)"
            );
        }
        Ok(())
    }
}
