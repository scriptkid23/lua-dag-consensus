//! Two-node smoke: certified vertex published on A is received on B (plan 06b-L1).

use std::time::Duration;

use consensus::event::Event;
use libp2p::Multiaddr;
use net::NetConfig;
use net::config::{GossipConfig, PeerConfig};
use net::deterministic_key::devnet_keypair_from_label;
use net::gossip_wire::encode_certified_vertex;
use net::swarm_runner::{GossipSpawn, spawn_gossip_tasks};
use dag::{cert, signing};
use node::devnet_keys::devnet_valset_four;
use tokio::sync::mpsc;
use types::{
    crypto_types::Hash32,
    dag::Vertex,
    primitives::Round,
};

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
async fn certified_vertex_round_trips_between_two_loopback_swarms() {
    let kp_b = devnet_keypair_from_label("l1-gossip-b").unwrap();
    let (_actions_b_tx, actions_b_rx) = mpsc::channel(8);
    let mut spawn_b = spawn_gossip_tasks(kp_b, loopback_cfg(vec![]), actions_b_rx, None)
        .await
        .expect("spawn B");
    wait_ready(&mut spawn_b).await;
    let b_addr = first_listen_addr(&spawn_b);
    let b_dial: Multiaddr = format!("{b_addr}/p2p/{}", spawn_b.local_peer_id)
        .parse()
        .expect("compose B dial multiaddr");

    let kp_a = devnet_keypair_from_label("l1-gossip-a").unwrap();
    let (_actions_a_tx, actions_a_rx) = mpsc::channel(8);
    let mut spawn_a =
        spawn_gossip_tasks(kp_a, loopback_cfg(vec![b_dial.to_string()]), actions_a_rx, None)
            .await
            .expect("spawn A");
    wait_ready(&mut spawn_a).await;

    tokio::time::sleep(Duration::from_millis(1500)).await;

    let valset = devnet_valset_four();
    let mut vertex = Vertex {
        round: Round(0),
        author: valset.entries[0].id,
        parents: vec![],
        blobs: vec![],
        hash: Hash32([0u8; 32]),
    };
    signing::seal_hash(&mut vertex);
    let cv = cert::build_quorum_cert(&vertex, &valset, &[0, 1, 2]).expect("quorum cert builds");
    let (topic, payload) = encode_certified_vertex(&cv).unwrap();
    spawn_a
        .publish_tx
        .send((topic, payload))
        .await
        .expect("publish certified vertex");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for CertifiedVertex on B"
        );
        match tokio::time::timeout(Duration::from_millis(500), spawn_b.events_rx.recv()).await {
            Ok(Some(Event::CertifiedVertexReceived(v))) => {
                assert_eq!(v, cv);
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
