//! Two-node L3 gossip smoke: MacroProposal crosses the wire (plan 06b-l3).

use std::time::Duration;

use consensus::action::Action;
use consensus::event::Event;
use libp2p::Multiaddr;
use net::NetConfig;
use net::config::{GossipConfig, PeerConfig};
use net::deterministic_key::devnet_keypair_from_label;
use net::swarm_runner::{GossipSpawn, spawn_gossip_tasks};
use tokio::sync::mpsc;
use types::crypto_types::{BlsSig, Hash32, VrfProof};
use types::macros::{MacroCheckpoint, MacroProposal};
use types::primitives::{Epoch, Height, ValidatorId};

fn loopback_cfg(bootstrap: Vec<String>) -> NetConfig {
    NetConfig {
        listen: vec!["/ip4/127.0.0.1/tcp/0".into()],
        bootstrap,
        gossip: GossipConfig {
            heartbeat_ms: 200,
            mesh_n: 4,
            mesh_n_low: 2,
            mesh_n_high: 6,
        },
        peers: PeerConfig {
            max_peers: 8,
            ban_duration_secs: 60,
        },
        macro_subnet_count: 0,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn macro_proposal_round_trips_between_two_loopback_swarms() {
    let kp_b = devnet_keypair_from_label("l3-smoke-b").unwrap();
    let (_actions_b_tx, actions_b_rx) = mpsc::channel::<Action>(8);
    let mut spawn_b = spawn_gossip_tasks(kp_b, loopback_cfg(vec![]), actions_b_rx, None)
        .await
        .expect("spawn B");
    wait_ready(&mut spawn_b).await;
    let b_addr = first_listen_addr(&spawn_b);
    let b_dial: Multiaddr = format!("{b_addr}/p2p/{}", spawn_b.local_peer_id)
        .parse()
        .expect("compose B dial multiaddr");

    let kp_a = devnet_keypair_from_label("l3-smoke-a").unwrap();
    let (actions_a_tx, actions_a_rx) = mpsc::channel::<Action>(8);
    let mut spawn_a =
        spawn_gossip_tasks(kp_a, loopback_cfg(vec![b_dial.to_string()]), actions_a_rx, None)
            .await
            .expect("spawn A");
    wait_ready(&mut spawn_a).await;

    tokio::time::sleep(Duration::from_millis(1500)).await;

    let proposal = MacroProposal {
        checkpoint: MacroCheckpoint {
            height: Height(1),
            epoch: Epoch(0),
            parent: Hash32::zero(),
            micro_root: Hash32([1; 32]),
            hash: Hash32([2; 32]),
        },
        proposer: ValidatorId([3; 32]),
        vrf_proof: VrfProof([4; 80]),
        proposer_sig: BlsSig([5; 96]),
    };
    actions_a_tx
        .send(Action::BroadcastMacroProposal(proposal.clone()))
        .await
        .unwrap();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for MacroProposal on B"
        );
        match tokio::time::timeout(Duration::from_millis(500), spawn_b.events_rx.recv()).await {
            Ok(Some(Event::MacroProposalReceived(p))) => {
                assert_eq!(p, proposal);
                return;
            }
            Ok(Some(_)) => {}
            Ok(None) => panic!("B events channel closed"),
            Err(_) => {}
        }
    }
}

async fn wait_ready(spawn: &mut GossipSpawn) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while !*spawn.ready.borrow() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "swarm did not become ready within 5s"
        );
        let _ = tokio::time::timeout(Duration::from_millis(250), spawn.ready.changed()).await;
    }
}

fn first_listen_addr(spawn: &GossipSpawn) -> Multiaddr {
    spawn
        .listen_addrs
        .borrow()
        .first()
        .cloned()
        .expect("listen_addrs populated when ready")
}
