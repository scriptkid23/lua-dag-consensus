//! Blob custody publish → store → availability smoke (07b).

use std::sync::Arc;

use dag::blob::commit::blob_id_from_payload;
use node::{
    blob::{BlobCustody, BlobCustodyConfig, RocksBlobStore},
    observability::metrics::Metrics,
};
use storage::{
    config::StorageConfig,
    db::Database,
    stores::blob_chunk_store,
};
use tokio::sync::mpsc;

fn open_temp_db() -> Database {
    let dir = tempfile::tempdir().unwrap();
    Database::open(&StorageConfig {
        path: dir.path().to_path_buf(),
        create_if_missing: true,
        max_total_wal_size_mb: 16,
    })
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn publish_marks_blob_available_and_persists_chunks() {
    let db = Arc::new(open_temp_db());
    let (publish_tx, mut publish_rx) = mpsc::channel(64);
    let (_chunks_tx, chunks_rx) = mpsc::channel(64);
    let metrics = Arc::new(Metrics::new().unwrap());

    tokio::spawn(async move {
        while publish_rx.recv().await.is_some() {}
    });

    let store_for_custody = Arc::new(RocksBlobStore::new(Arc::clone(&db)))
        as Arc<dyn dag::blob::store::BlobStore>;
    let handle = BlobCustody::spawn(
        store_for_custody,
        chunks_rx,
        publish_tx,
        BlobCustodyConfig {
            chunk_size: 65_536,
            erasure: None,
        },
        metrics,
    );

    let payload = vec![0xBEu8; 100_000];
    let expected_id = blob_id_from_payload(&payload);
    let blob_id = handle.publish_payload(payload).await.expect("publish");
    assert_eq!(blob_id, expected_id);
    assert!(handle.is_available(&blob_id));
    assert!(blob_chunk_store::has(&db, &blob_id, 0).unwrap());
    assert!(blob_chunk_store::has(&db, &blob_id, 1).unwrap());
}
