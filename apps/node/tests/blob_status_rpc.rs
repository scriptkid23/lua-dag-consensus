//! `lua_getBlobStatus` reports the consensus tier plus local custody
//! availability read from the custody ledger (layer-1 diagram edge).

use std::sync::Arc;

use node::{
    blob::{BlobCustody, BlobCustodyConfig, BlobCustodyHandle, RocksBlobStore},
    live_dag::LiveDag,
    observability::metrics::Metrics,
    query::RocksConsensusQuery,
    rpc_server::blob_status_at,
};
use storage::{RocksPersistence, config::StorageConfig, db::Database};
use tokio::sync::mpsc;

fn open_query(dir: &tempfile::TempDir) -> (Arc<Database>, RocksConsensusQuery) {
    let db = Arc::new(
        Database::open(&StorageConfig {
            path: dir.path().to_path_buf(),
            create_if_missing: true,
            max_total_wal_size_mb: 16,
        })
        .unwrap(),
    );
    let query = RocksConsensusQuery::new(
        RocksPersistence::new(Arc::clone(&db)),
        Arc::new(LiveDag::new(Arc::clone(&db))),
    );
    (db, query)
}

fn spawn_custody(db: &Arc<Database>) -> BlobCustodyHandle {
    let store =
        Arc::new(RocksBlobStore::new(Arc::clone(db))) as Arc<dyn dag::blob::store::BlobStore>;
    let (_chunks_tx, chunks_rx) = mpsc::channel(64);
    let (publish_tx, mut publish_rx) = mpsc::channel(256);
    tokio::spawn(async move { while publish_rx.recv().await.is_some() {} });
    BlobCustody::spawn(
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
        Arc::new(Metrics::new().unwrap()),
    )
}

fn params_for(id: [u8; 32]) -> serde_json::Value {
    serde_json::json!([format!("0x{}", hex::encode(id))])
}

#[tokio::test(flavor = "multi_thread")]
async fn locally_available_true_after_local_publish() {
    let dir = tempfile::tempdir().unwrap();
    let (db, query) = open_query(&dir);
    let custody = spawn_custody(&db);

    let blob_id = custody.publish_payload(vec![0xC7; 1500]).await.unwrap();

    let resp = blob_status_at(&query, &Some(custody), &params_for(blob_id.0));
    assert_eq!(resp["status"], "accepted");
    assert_eq!(resp["locally_available"], true);
}

#[tokio::test(flavor = "multi_thread")]
async fn locally_available_false_for_unknown_blob() {
    let dir = tempfile::tempdir().unwrap();
    let (db, query) = open_query(&dir);
    let custody = spawn_custody(&db);

    let resp = blob_status_at(&query, &Some(custody), &params_for([0u8; 32]));
    assert_eq!(resp["status"], "accepted");
    assert_eq!(resp["locally_available"], false);
}

#[test]
fn locally_available_null_when_custody_disabled() {
    let dir = tempfile::tempdir().unwrap();
    let (_db, query) = open_query(&dir);

    let resp = blob_status_at(&query, &None, &params_for([0u8; 32]));
    assert_eq!(resp["status"], "accepted");
    assert_eq!(resp["locally_available"], serde_json::Value::Null);
}
