//! Single-node smoke: L1 driver + orchestrator emit `BroadcastMicroQc`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use consensus::{StateMachine, action::Action};
use net::Bridge;
use node::{
    action_applier::ActionApplier,
    devnet_keys::{devnet_valset_four, validator_id_from_label},
    host_context::StubHostBundle,
    l1::L1Driver,
    live_dag::LiveDag,
    observability::metrics::Metrics,
    orchestrator::Orchestrator,
    timer::TimerRegistry,
};
use storage::{Database, RocksPersistence, config::StorageConfig};
use tokio::sync::mpsc;

#[tokio::test(flavor = "multi_thread")]
async fn l1_driver_advances_bullshark_to_micro_qc_broadcast() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(
        Database::open(&StorageConfig {
            path: dir.path().to_path_buf(),
            create_if_missing: true,
            max_total_wal_size_mb: 16,
        })
        .unwrap(),
    );
    let live_dag = Arc::new(LiveDag::new(Arc::clone(&db)));
    let persistence = RocksPersistence::new(db);
    let valset = devnet_valset_four();
    let cfg = consensus::Config::default_table_17_1();
    let self_id = validator_id_from_label("node0");

    let (events_tx, events_rx) = mpsc::channel(1024);
    let (bridge, _bridge_handle) = Bridge::with_channels(events_tx.clone(), 1024);
    let (net_actions_tx, mut net_actions_rx) = mpsc::channel(1024);
    let (publish_tx, mut publish_rx) = mpsc::channel(64);
    tokio::spawn(async move {
        while publish_rx.recv().await.is_some() {}
    });

    let timer_registry = Arc::new(TimerRegistry::default());
    let (timer_schedule_tx, mut timer_schedule_rx) = mpsc::channel(256);
    let events_tx_timer = events_tx.clone();
    let registry_for_loop = timer_registry.clone();
    tokio::spawn(async move {
        while let Some((id, delay)) = timer_schedule_rx.recv().await {
            node::timer::schedule_event(
                &registry_for_loop,
                events_tx_timer.clone(),
                id,
                delay,
            );
        }
    });

    let metrics = Arc::new(Metrics::new().expect("metrics"));
    let sm = StateMachine::new(cfg.clone(), self_id);
    let host_bundle =
        StubHostBundle::new("node0", valset.clone(), Arc::clone(&live_dag), None, None).unwrap();
    let beacon = Arc::clone(&host_bundle.beacon);
    let action_applier = ActionApplier::new(
        persistence.clone(),
        timer_schedule_tx,
        timer_registry,
        beacon.clone(),
        metrics.clone(),
    );

    let orch = Orchestrator::new(
        sm,
        bridge,
        events_rx,
        persistence,
        metrics.clone(),
        net_actions_tx,
        host_bundle,
        action_applier,
        valset.clone(),
        true,
        false,
    );
    tokio::spawn(orch.run());

    let driver = L1Driver::new(
        valset,
        cfg,
        live_dag,
        beacon,
        events_tx,
        publish_tx,
        Duration::from_millis(50),
        true,
        None,
        metrics.clone(),
    );
    tokio::spawn(driver.run());

    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for BroadcastMicroQc from L1-driven Bullshark path"
        );
        tokio::select! {
            action = net_actions_rx.recv() => {
                let Some(action) = action else { break; };
                if matches!(action, Action::BroadcastMicroQc(_)) {
                    return;
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(25)) => {}
        }
    }
    panic!("net actions channel closed without BroadcastMicroQc");
}
