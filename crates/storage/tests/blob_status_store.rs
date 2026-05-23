//! Blob status column family roundtrip and monotonic rules.

use consensus::api::tier::BlobStatus;
use storage::{
    config::StorageConfig,
    db::Database,
    stores::blob_status_store::{get, put_monotonic},
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
fn put_and_get_roundtrip() {
    let db = open_temp_db();
    let blob = BlobId([7; 32]);
    put_monotonic(&db, &blob, BlobStatus::Justified).unwrap();
    assert_eq!(get(&db, &blob).unwrap(), Some(BlobStatus::Justified));
}

#[test]
fn monotonic_no_downgrade() {
    let db = open_temp_db();
    let blob = BlobId([8; 32]);
    put_monotonic(&db, &blob, BlobStatus::Finalized).unwrap();
    put_monotonic(&db, &blob, BlobStatus::SoftConfirmed).unwrap();
    assert_eq!(get(&db, &blob).unwrap(), Some(BlobStatus::Finalized));
}

#[test]
fn upgrade_allowed() {
    let db = open_temp_db();
    let blob = BlobId([9; 32]);
    put_monotonic(&db, &blob, BlobStatus::SoftConfirmed).unwrap();
    put_monotonic(&db, &blob, BlobStatus::Justified).unwrap();
    assert_eq!(get(&db, &blob).unwrap(), Some(BlobStatus::Justified));
}

#[test]
fn missing_key_returns_none() {
    let db = open_temp_db();
    assert_eq!(get(&db, &BlobId([0; 32])).unwrap(), None);
}
