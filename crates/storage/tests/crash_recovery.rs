//! Open → write → drop → reopen → read.

use std::sync::Arc;

use consensus::ports::Persistence;
use storage::{Database, RocksPersistence, StorageConfig};
use tempfile::tempdir;
use types::{
    crypto_types::{BlsAggSig, BlsSig, Hash32},
    macros::{AggregationMode, MacroQc},
};

#[test]
fn reopen_recovers_written_macro_qc() {
    let dir = tempdir().unwrap();
    let cfg = StorageConfig {
        path: dir.path().to_path_buf(),
        create_if_missing: true,
        max_total_wal_size_mb: 16,
    };
    {
        let db = Arc::new(Database::open(&cfg).unwrap());
        let p = RocksPersistence::new(db);
        let qc = MacroQc {
            checkpoint_hash: Hash32([5; 32]),
            mode: AggregationMode::Mode0Flat,
            agg: BlsAggSig {
                sig: BlsSig([0; 96]),
                bitmap: vec![0xFF],
            },
        };
        p.store_macro_qc(&qc).unwrap();
    }
    let db2 = Arc::new(Database::open(&cfg).unwrap());
    let p2 = RocksPersistence::new(db2);
    let got = p2.macro_qc_for(&Hash32([5; 32])).unwrap();
    assert!(got.is_some());
}
