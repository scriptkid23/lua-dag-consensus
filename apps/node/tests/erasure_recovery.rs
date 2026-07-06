//! Erasure publish stores n shards and marks blob available (07c).

use std::sync::Arc;

use dag::erasure::ErasureConfig;
use node::{
    blob::{BlobCustody, BlobCustodyConfig, RocksBlobStore},
    observability::metrics::Metrics,
};
use storage::{config::StorageConfig, db::Database};
use tokio::sync::mpsc;

#[tokio::test(flavor = "multi_thread")]
async fn erasure_publish_stores_all_shards_and_lists_chunk_refs() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(
        Database::open(&StorageConfig {
            path: dir.path().to_path_buf(),
            create_if_missing: true,
            max_total_wal_size_mb: 16,
        })
        .unwrap(),
    );
    let cfg = ErasureConfig::devnet_default();
    let (publish_tx, mut publish_rx) = mpsc::channel(64);
    let (_chunks_tx, chunks_rx) = mpsc::channel(64);
    let metrics = Arc::new(Metrics::new().unwrap());

    tokio::spawn(async move {
        while publish_rx.recv().await.is_some() {}
    });

    let store = Arc::new(RocksBlobStore::new(Arc::clone(&db))) as Arc<dyn dag::blob::store::BlobStore>;
    let handle = BlobCustody::spawn(
        store,
        chunks_rx,
        publish_tx,
        BlobCustodyConfig {
            erasure: cfg,
        },
        metrics,
    );

    let payload = vec![0xEFu8; 100_000];
    let blob_id = handle.publish_payload(payload).await.expect("publish");
    assert!(handle.is_available(&blob_id));
    let refs = handle.list_chunk_refs(&blob_id).expect("list");
    assert_eq!(refs.len(), usize::try_from(cfg.n).unwrap());
}
