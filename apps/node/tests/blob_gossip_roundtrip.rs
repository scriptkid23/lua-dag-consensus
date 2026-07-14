//! Two-node blob chunk gossip roundtrip (07b).

use std::sync::Arc;
use std::time::Duration;

use dag::blob::chunk::erasure_chunks;
use dag::erasure::{encode_shards, ErasureConfig};
use net::NetConfig;
use net::config::{GossipConfig, PeerConfig};
use net::deterministic_key::devnet_keypair_from_label;
use net::gossip_wire::encode_blob_chunk;
use net::swarm_runner::{GossipSpawn, spawn_gossip_tasks};
use node::{
    blob::{BlobCustody, BlobCustodyConfig, RocksBlobStore},
    observability::metrics::Metrics,
};
use storage::{config::StorageConfig, db::Database};
use tokio::sync::mpsc;

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
async fn blob_chunks_roundtrip_between_two_loopback_swarms() {
    let db = Arc::new({
        let dir = tempfile::tempdir().unwrap();
        Database::open(&StorageConfig {
            path: dir.path().to_path_buf(),
            create_if_missing: true,
            max_total_wal_size_mb: 16,
        })
        .unwrap()
    });

    let (blob_tx_b, blob_rx_b) = mpsc::channel(64);
    let kp_b = devnet_keypair_from_label("blob-gossip-b").unwrap();
    let (_actions_b_tx, actions_b_rx) = mpsc::channel(8);
    let mut spawn_b = spawn_gossip_tasks(
        kp_b,
        loopback_cfg(vec![]),
        actions_b_rx,
        Some(blob_tx_b),
    )
    .await
    .expect("spawn B");
    wait_ready(&mut spawn_b).await;
    let b_addr = first_listen_addr(&spawn_b);
    let b_dial = format!("{b_addr}/p2p/{}", spawn_b.local_peer_id);

    let kp_a = devnet_keypair_from_label("blob-gossip-a").unwrap();
    let (_actions_a_tx, actions_a_rx) = mpsc::channel(8);
    let mut spawn_a = spawn_gossip_tasks(
        kp_a,
        loopback_cfg(vec![b_dial]),
        actions_a_rx,
        None,
    )
    .await
    .expect("spawn A");
    wait_ready(&mut spawn_a).await;

    tokio::time::sleep(Duration::from_millis(1500)).await;

    let store = Arc::new(RocksBlobStore::new(Arc::clone(&db)));
    let metrics = Arc::new(Metrics::new().unwrap());
    let custody = BlobCustody::spawn(
        store,
        blob_rx_b,
        spawn_b.publish_tx.clone(),
        BlobCustodyConfig {
            erasure: ErasureConfig::devnet_default(),
        },
        metrics,
    );

    let payload = vec![0xCDu8; 100_000];
    let blob_id = dag::blob::commit::blob_id_from_payload(&payload);
    let cfg = ErasureConfig::devnet_default();
    let shards = encode_shards(&payload, &cfg).unwrap();
    let chunks = erasure_chunks(blob_id, payload.len() as u64, &shards);
    for chunk in &chunks {
        let (topic, bytes) = encode_blob_chunk(chunk).unwrap();
        spawn_a
            .publish_tx
            .send((topic, bytes))
            .await
            .expect("publish chunk");
    }

    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while tokio::time::Instant::now() < deadline {
        if custody.is_available(&blob_id) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    panic!("timed out waiting for blob custody on B");
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

fn first_listen_addr(spawn: &GossipSpawn) -> libp2p::Multiaddr {
    spawn
        .listen_addrs
        .borrow()
        .first()
        .cloned()
        .expect("listen_addrs populated when ready")
}
