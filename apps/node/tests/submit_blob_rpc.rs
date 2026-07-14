//! `lua_submitBlob` handler: erasure-only publish and explicit oversize rejection.

use std::sync::Arc;

use dag::erasure::ErasureConfig;
use node::{
    blob::{BlobCustody, BlobCustodyConfig, BlobCustodyHandle, RocksBlobStore},
    observability::metrics::Metrics,
    rpc_server::submit_blob,
};
use storage::{config::StorageConfig, db::Database};
use tokio::sync::mpsc;

fn spawn_custody(dir: &tempfile::TempDir) -> BlobCustodyHandle {
    let db = Arc::new(
        Database::open(&StorageConfig {
            path: dir.path().to_path_buf(),
            create_if_missing: true,
            max_total_wal_size_mb: 16,
        })
        .unwrap(),
    );
    let store = Arc::new(RocksBlobStore::new(db));
    let (_chunks_tx, chunks_rx) = mpsc::channel(64);
    let (publish_tx, mut publish_rx) = mpsc::channel(256);
    tokio::spawn(async move { while publish_rx.recv().await.is_some() {} });
    BlobCustody::spawn(
        store,
        chunks_rx,
        publish_tx,
        BlobCustodyConfig {
            erasure: ErasureConfig {
                k: 4,
                n: 8,
                data_shard_size: 1024,
            },
        },
        Arc::new(Metrics::new().unwrap()),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn submit_rejects_payload_over_erasure_capacity() {
    let dir = tempfile::tempdir().unwrap();
    let custody = spawn_custody(&dir);
    // capacity = k * data_shard_size = 4096 bytes; send 5000.
    let params = serde_json::json!({ "payload_hex": hex::encode(vec![0xAAu8; 5000]) });
    let resp = submit_blob(&Some(custody), &params).await;
    assert_eq!(resp["error"], "payload exceeds max blob size (4096 bytes)");
}

#[tokio::test(flavor = "multi_thread")]
async fn submit_within_capacity_returns_blob_id_and_shard_count() {
    let dir = tempfile::tempdir().unwrap();
    let custody = spawn_custody(&dir);
    let params = serde_json::json!({ "payload_hex": hex::encode(vec![0xBBu8; 1500]) });
    let resp = submit_blob(&Some(custody), &params).await;
    assert!(resp["blob_id"].as_str().unwrap().starts_with("0x"));
    assert_eq!(resp["chunk_count"], 8);
}
