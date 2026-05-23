//! Top-level binary wiring.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use consensus::{StateMachine, action::Action};
use net::Bridge;
use storage::{Database, RocksPersistence};
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};

use crate::{
    action_applier::ActionApplier,
    args::Args,
    config::NodeConfig,
    devnet_keys::validator_id_from_label,
    host_context::{ChainedBeacon, StubHostBundle},
    l1::L1Driver,
    live_dag::LiveDag,
    observability::{health, metrics::Metrics, tracing as tracing_init},
    orchestrator::Orchestrator,
    query::RocksConsensusQuery,
    rpc_server, shutdown,
    timer::{TimerRegistry, TokioClock},
    validator_set_loader,
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

fn resolve_valset_path(config_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    if path.starts_with("config/") {
        if let Some(root) = config_dir.parent() {
            return root.join(path);
        }
    }
    config_dir.join(path)
}

async fn run_async(cfg: NodeConfig, args: Args) -> Result<()> {
    info!(target: "node", "starting LUA-DAG node");

    // L3 host wiring complete in 06b-l3; L1 ingress via gossip (06b-L1).
    if cfg.node.network_mode == "live"
        && !args.allow_skeleton_network
        && !cfg.node.l3_wire_complete
    {
        anyhow::bail!(
            "network_mode=\"live\" requires --allow-skeleton-network until L3 node \
             production wiring is complete (set node.l3_wire_complete or pass the flag)"
        );
    }

    let valset_path = resolve_valset_path(&args.config_dir, &cfg.node.validator_set_path);
    let valset = validator_set_loader::load_from_toml(&valset_path)
        .with_context(|| format!("load validator set from {}", valset_path.display()))?;
    let self_id = validator_id_from_label(&cfg.node.identity.label);
    valset
        .entries
        .iter()
        .find(|e| e.id == self_id)
        .with_context(|| format!("self_id {self_id} not in validator set"))?;

    // Storage.
    let db = Arc::new(Database::open(&cfg.storage)?);
    let persistence = RocksPersistence::new(Arc::clone(&db));
    let live_dag = Arc::new(LiveDag::new(db));

    // Observability.
    let metrics = Arc::new(Metrics::new()?);
    let sm: StateMachine = StateMachine::new(cfg.consensus.clone(), self_id);
    let _clock = TokioClock::new();

    // Bridge (events_tx fed into consensus; bridge.actions_rx is drained but unused
    // in the live path now — broadcasts go via the swarm channel).
    let (events_tx, events_rx) = mpsc::channel(1024);
    let (bridge, _bridge_handle) = Bridge::with_channels(events_tx.clone(), 1024);

    // Timers → SM events.
    let timer_registry = Arc::new(TimerRegistry::default());
    let (timer_schedule_tx, mut timer_schedule_rx) = mpsc::channel(256);
    let events_tx_timer = events_tx.clone();
    let registry_for_loop = timer_registry.clone();
    tokio::spawn(async move {
        while let Some((id, delay)) = timer_schedule_rx.recv().await {
            crate::timer::schedule_event(
                &registry_for_loop,
                events_tx_timer.clone(),
                id,
                delay,
            );
        }
    });

    let host_bundle = StubHostBundle::new(
        &cfg.node.identity.label,
        valset.clone(),
        Arc::clone(&live_dag),
        None,
    )
    .context("build host context bundle")?;
    let beacon: Arc<ChainedBeacon> = Arc::clone(&host_bundle.beacon);
    let action_applier = ActionApplier::new(
        persistence.clone(),
        timer_schedule_tx,
        timer_registry,
        beacon,
        metrics.clone(),
    );

    // Consensus SM (after host ports are ready).
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // ─── Live swarm ────────────────────────────────────────────────────
    let (net_actions_tx, net_actions_rx) = mpsc::channel::<Action>(1024);
    let mut gossip_publish_tx: Option<mpsc::Sender<(net::gossip::Topic, Vec<u8>)>> = None;
    let (swarm_handle, net_ready_rx) = if args.allow_skeleton_network
        && cfg.node.network_mode != "live"
    {
        let (ready_tx, ready_rx) = watch::channel(true);
        drop(ready_tx);
        (None, ready_rx)
    } else {
        let mut net_cfg = cfg.net.clone();
        if net_cfg.macro_subnet_count == 0 {
            let n_e = u32::try_from(valset.entries.len()).unwrap_or(0);
            net_cfg.macro_subnet_count =
                consensus::macro_fin::compute_ke(&cfg.consensus, n_e).0;
        }
        let keypair = net::deterministic_key::devnet_keypair_from_label(&cfg.node.identity.label)
            .context("derive devnet keypair from node.identity.label")?;
        let spawn = net::swarm_runner::spawn_gossip_tasks(keypair, net_cfg, net_actions_rx)
            .await
            .context("spawn gossipsub swarm")?;
        gossip_publish_tx = Some(spawn.publish_tx);

        let mut events_rx_swarm = spawn.events_rx;
        let events_tx_for_swarm = events_tx.clone();
        let metrics_fanin = metrics.clone();
        tokio::spawn(async move {
            while let Some(ev) = events_rx_swarm.recv().await {
                match events_tx_for_swarm.try_send(ev) {
                    Ok(()) => {}
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        metrics_fanin.events_dropped.inc();
                        warn!(
                            target: "node::runtime",
                            "consensus events channel full; dropping inbound gossip event",
                        );
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => break,
                }
            }
        });

        (Some(spawn.handle), spawn.ready)
    };

    let query = Arc::new(RocksConsensusQuery::new(persistence.clone()));

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
    rpc_server::serve(&cfg.rpc_listen, query, rpc_shutdown).await?;

    if cfg.node.l1_driver_enabled {
        let publish_tx = gossip_publish_tx.with_context(|| {
            "l1_driver_enabled requires a live gossip swarm (not skeleton network mode)"
        })?;
        let round_ms = cfg.consensus.timing.round_duration_ms;
        let driver = L1Driver::new(
            valset.clone(),
            cfg.consensus.clone(),
            Arc::clone(&live_dag),
            Arc::clone(&host_bundle.beacon),
            events_tx.clone(),
            publish_tx,
            std::time::Duration::from_millis(round_ms),
        );
        tokio::spawn(async move {
            driver.run().await;
        });
        info!(target: "node", round_duration_ms = round_ms, "L1 driver started");
    }

    // Orchestrator.
    let orch = Orchestrator::new(
        sm,
        bridge,
        events_rx,
        persistence,
        metrics,
        net_actions_tx,
        host_bundle,
        action_applier,
    );
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

        if cfg.node.network_mode == "live"
            && !args.allow_skeleton_network
            && !cfg.node.l3_wire_complete
        {
            anyhow::bail!(
                "network_mode=\"live\" requires --allow-skeleton-network until L3 node \
                 production wiring is complete (set node.l3_wire_complete or pass the flag)"
            );
        }
        Ok(())
    }
}
