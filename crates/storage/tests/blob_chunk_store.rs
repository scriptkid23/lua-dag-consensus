//! Blob chunk column family roundtrip.

use storage::{
    config::StorageConfig,
    db::Database,
    stores::blob_chunk_store::{get, has, put},
};
use tempfile::tempdir;
use types::primitives::BlobId;

fn open_temp_db() -> Database {
    let dir = tempdir().unwrap();
    Database::open(&StorageConfig {
        path: dir.path().to_path_buf(),
        create_if_missing: true,
        max_total_wal_size_mb: 16,
    })
    .unwrap()
}

#[test]
fn put_get_has_roundtrip() {
    let db = open_temp_db();
    let blob = BlobId([0xAA; 32]);
    put(&db, &blob, 0, 2, 100_000, &[1, 2, 3]).unwrap();
    assert!(has(&db, &blob, 0).unwrap());
    assert!(!has(&db, &blob, 1).unwrap());
    assert_eq!(get(&db, &blob, 0).unwrap(), Some(vec![1, 2, 3]));
}
