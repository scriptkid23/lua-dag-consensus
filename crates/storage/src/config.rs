//! Storage configuration.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Top-level storage config.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StorageConfig {
    /// On-disk path for the `RocksDB` instance.
    pub path: PathBuf,
    /// Create the directory if missing on open.
    pub create_if_missing: bool,
    /// Maximum number of WAL files retained per column family.
    pub max_total_wal_size_mb: u64,
}

impl StorageConfig {
    /// Local-devnet defaults under `./data`.
    #[must_use]
    pub fn devnet_default() -> Self {
        Self {
            path: PathBuf::from("./data/rocksdb"),
            create_if_missing: true,
            max_total_wal_size_mb: 256,
        }
    }
}
