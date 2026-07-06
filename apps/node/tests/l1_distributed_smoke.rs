//! Distributed L1 smoke: orchestrator genesis-proposes, two injected peer
//! partials complete the quorum, the cert broadcasts and self-ingests.

use std::sync::Arc;
use std::time::{Duration, Instant};

use consensus::{StateMachine, action::Action, event::Event, ports::DagView};
use crypto::hash::dst;
use net::Bridge;
use node::{
    action_applier::ActionApplier,
    blob::{BlobCustody, BlobCustodyConfig, RocksBlobStore},
    devnet_keys::{devnet_bls_ikm, devnet_valset_four, validator_id_from_label},
    host_context::StubHostBundle,
    live_dag::LiveDag,
    observability::metrics::Metrics,
    orchestrator::Orchestrator,
    timer::TimerRegistry,
};
use storage::{Database, RocksPersistence, config::StorageConfig};
use tokio::sync::mpsc;
use types::{
    dag::{VertexPartial, VertexProposal},
    primitives::Round,
};

struct Node0 {
    net_actions_rx: mpsc::Receiver<Action>,
    events_tx: mpsc::Sender<Event>,
    live_dag: Arc<LiveDag>,
    _dir: tempfile::TempDir,
}

async fn spawn_node0(custody_blob: Option<Vec<u8>>) -> Node0 {
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
    let persistence = RocksPersistence::new(Arc::clone(&db));
    let valset = devnet_valset_four();
    let cfg = consensus::Config::default_table_17_1();
    let self_id = validator_id_from_label("node0");

    let (events_tx, events_rx) = mpsc::channel(1024);
    let (bridge, _bridge_handle) = Bridge::with_channels(events_tx.clone(), 1024);
    let (net_actions_tx, net_actions_rx) = mpsc::channel(1024);
    let metrics = Arc::new(Metrics::new().unwrap());

    let custody = if let Some(payload) = custody_blob {
        let store = Arc::new(RocksBlobStore::new(Arc::clone(&db)))
            as Arc<dyn dag::blob::store::BlobStore>;
        let (_chunks_tx, chunks_rx) = mpsc::channel(64);
        let (publish_tx, mut publish_rx) = mpsc::channel(256);
        tokio::spawn(async move { while publish_rx.recv().await.is_some() {} });
        let handle = BlobCustody::spawn(
            store,
            chunks_rx,
            publish_tx,
            BlobCustodyConfig {
                erasure: dag::erasure::ErasureConfig {
                    k: 4,
                    n: 8,
                    data_shard_size: 1024,
                },
            },
            metrics.clone(),
        );
        handle.publish_payload(payload).await.unwrap();
        Some(handle)
    } else {
        None
    };

    let timer_registry = Arc::new(TimerRegistry::default());
    let (timer_schedule_tx, mut timer_schedule_rx) = mpsc::channel(256);
    let events_tx_timer = events_tx.clone();
    let registry_for_loop = timer_registry.clone();
    tokio::spawn(async move {
        while let Some((id, delay)) = timer_schedule_rx.recv().await {
            node::timer::schedule_event(&registry_for_loop, events_tx_timer.clone(), id, delay);
        }
    });

    let sm = StateMachine::new(cfg, self_id);
    let host_bundle =
        StubHostBundle::new("node0", valset.clone(), Arc::clone(&live_dag), None, custody)
            .unwrap();
    let beacon = Arc::clone(&host_bundle.beacon);
    let action_applier = ActionApplier::new(
        persistence.clone(),
        timer_schedule_tx,
        timer_registry,
        beacon,
        metrics.clone(),
    );
    let orch = Orchestrator::new(
        sm,
        bridge,
        events_rx,
        persistence,
        metrics,
        net_actions_tx,
        host_bundle,
        action_applier,
        valset,
        true,
    );
    tokio::spawn(orch.run());
    Node0 {
        net_actions_rx,
        events_tx,
        live_dag,
        _dir: dir,
    }
}

async fn next_action<F: Fn(&Action) -> bool>(
    rx: &mut mpsc::Receiver<Action>,
    want: F,
    what: &str,
) -> Action {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        match tokio::time::timeout(Duration::from_millis(250), rx.recv()).await {
            Ok(Some(a)) if want(&a) => return a,
            Ok(Some(_)) => {}
            Ok(None) => panic!("net actions channel closed waiting for {what}"),
            Err(_) => {}
        }
    }
}

fn peer_partial(label: &str, proposal: &VertexProposal) -> VertexPartial {
    let sk = crypto::bls::SecretKey::from_ikm(&devnet_bls_ikm(label)).unwrap();
    let msg = dag::signing::signing_bytes(&proposal.vertex);
    VertexPartial {
        vertex_hash: proposal.vertex.hash,
        round: proposal.vertex.round,
        author: proposal.vertex.author,
        voter: validator_id_from_label(label),
        sig: crypto::bls::sign::sign(&sk, dst::VERTEX_CERT, &msg),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn genesis_proposal_plus_two_peer_partials_yield_verified_cert() {
    let mut node = spawn_node0(None).await;
    let valset = devnet_valset_four();

    let action = next_action(
        &mut node.net_actions_rx,
        |a| matches!(a, Action::BroadcastVertexProposal(_)),
        "genesis proposal",
    )
    .await;
    let Action::BroadcastVertexProposal(proposal) = action else {
        unreachable!()
    };
    assert_eq!(proposal.vertex.round, Round(0));
    assert_eq!(proposal.vertex.author, validator_id_from_label("node0"));
    assert!(proposal.vertex.parents.is_empty());

    let mut forged = peer_partial("node1", &proposal);
    forged.sig = types::crypto_types::BlsSig([0xEE; 96]);
    node.events_tx
        .send(Event::VertexPartialReceived(forged))
        .await
        .unwrap();
    node.events_tx
        .send(Event::VertexPartialReceived(peer_partial("node1", &proposal)))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        node.live_dag.vertices_at_round(Round(0)).unwrap().is_empty(),
        "no cert below quorum / from forged partials"
    );

    node.events_tx
        .send(Event::VertexPartialReceived(peer_partial("node2", &proposal)))
        .await
        .unwrap();
    let action = next_action(
        &mut node.net_actions_rx,
        |a| matches!(a, Action::BroadcastCertifiedVertex(_)),
        "certified vertex",
    )
    .await;
    let Action::BroadcastCertifiedVertex(cv) = action else {
        unreachable!()
    };
    dag::cert::verify_certified_vertex(&cv, &valset).expect("broadcast cert verifies");
    assert_eq!(cv.vertex.hash, proposal.vertex.hash);

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if !node.live_dag.vertices_at_round(Round(0)).unwrap().is_empty() {
            break;
        }
        assert!(Instant::now() < deadline, "own cert never self-ingested");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn pending_blob_rides_in_genesis_proposal() {
    let mut node = spawn_node0(Some(vec![0xA5; 1500])).await;
    let action = next_action(
        &mut node.net_actions_rx,
        |a| matches!(a, Action::BroadcastVertexProposal(_)),
        "genesis proposal with blob",
    )
    .await;
    let Action::BroadcastVertexProposal(proposal) = action else {
        unreachable!()
    };
    assert_eq!(proposal.vertex.blobs.len(), 1, "drained pending BlobRef");
}
