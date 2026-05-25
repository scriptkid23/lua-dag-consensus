//! Two-swarm loopback smoke (spec §8 acceptance bottom-up):
//!
//!   node A publishes a `MicroQc` → node B receives `Event::MicroQcAssembled`.
//!
//! Runs over `127.0.0.1:0` TCP with no QUIC and no Docker. The full
//! 4-node Compose smoke happens in CI (spec §8); this test is the
//! smallest credible exercise of the live swarm pipeline.

use std::time::Duration;

use consensus::action::Action;
use consensus::event::Event;
use libp2p::Multiaddr;
use net::NetConfig;
use net::config::{GossipConfig, PeerConfig};
use net::deterministic_key::devnet_keypair_from_label;
use net::swarm_runner::{GossipSpawn, spawn_gossip_tasks};
use tokio::sync::mpsc;
use types::crypto_types::{BlsAggSig, BlsSig, Hash32};
use types::micro::MicroQc;

fn loopback_cfg(bootstrap: Vec<String>) -> NetConfig {
    NetConfig {
        listen: vec!["/ip4/127.0.0.1/tcp/0".into()],
        bootstrap,
        gossip: GossipConfig {
            // Gossipsub's `mesh_outbound_min` default is 2; `mesh_n_low` must
            // therefore be ≥ 2 even on a two-node loopback mesh.
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
async fn micro_qc_round_trips_between_two_loopback_swarms() {
    // ─── receiver (node B) — binds first so A can dial it ──────────────
    let kp_b = devnet_keypair_from_label("loopback-b").unwrap();
    let (_actions_b_tx, actions_b_rx) = mpsc::channel::<Action>(8);
    let mut spawn_b = spawn_gossip_tasks(kp_b, loopback_cfg(vec![]), actions_b_rx, None)
        .await
        .expect("spawn B");
    wait_ready(&mut spawn_b).await;
    let b_addr = first_listen_addr(&spawn_b);
    let b_dial: Multiaddr = format!("{b_addr}/p2p/{}", spawn_b.local_peer_id)
        .parse()
        .expect("compose B dial multiaddr");

    // ─── publisher (node A) — dials B as bootstrap ─────────────────────
    let kp_a = devnet_keypair_from_label("loopback-a").unwrap();
    let (actions_a_tx, actions_a_rx) = mpsc::channel::<Action>(8);
    let mut spawn_a =
        spawn_gossip_tasks(kp_a, loopback_cfg(vec![b_dial.to_string()]), actions_a_rx, None)
            .await
            .expect("spawn A");
    wait_ready(&mut spawn_a).await;

    // Give gossipsub heartbeats time to form a mesh between A and B.
    tokio::time::sleep(Duration::from_millis(1500)).await;

    let m = MicroQc {
        checkpoint_hash: Hash32([7u8; 32]),
        agg: BlsAggSig {
            sig: BlsSig([0u8; 96]),
            bitmap: vec![0xFF],
        },
    };
    actions_a_tx
        .send(Action::BroadcastMicroQc(m.clone()))
        .await
        .unwrap();

    // Expect B to receive `Event::MicroQcAssembled` within 10s.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for MicroQc to arrive on B"
        );
        match tokio::time::timeout(Duration::from_millis(500), spawn_b.events_rx.recv()).await {
            Ok(Some(Event::MicroQcAssembled(m2))) => {
                assert_eq!(m, m2, "MicroQc payload drifted on the wire");
                return;
            }
            Ok(Some(_other)) => {}
            Ok(None) => panic!("B's events channel closed"),
            // Heartbeat-driven mesh formation can take a few cycles; the
            // sender's gossipsub will be repeatedly heart-beated by libp2p,
            // so we just keep polling until the deadline.
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
    let addrs = spawn.listen_addrs.borrow();
    addrs
        .first()
        .cloned()
        .expect("listen_addrs must be populated by the time ready=true")
}
