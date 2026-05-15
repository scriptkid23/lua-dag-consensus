//! Smoke test: configure + boot core components without binding ports.

use std::sync::Arc;

use storage::{Database, RocksPersistence};
use tempfile::tempdir;

#[test]
fn open_storage_and_construct_persistence() {
    let dir = tempdir().unwrap();
    let cfg = storage::StorageConfig {
        path: dir.path().to_path_buf(),
        create_if_missing: true,
        max_total_wal_size_mb: 16,
    };
    let db = Arc::new(Database::open(&cfg).unwrap());
    let _p = RocksPersistence::new(db);
}
