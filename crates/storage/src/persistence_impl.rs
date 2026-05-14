//! `RocksPersistence`: concrete impl of [`consensus::ports::Persistence`].

use std::sync::Arc;

use consensus::ports::Persistence;
use types::{
    crypto_types::Hash32,
    macros::{MacroCheckpoint, MacroQc},
    micro::MicroQc,
    primitives::Height,
    slashing::SlashEvidence,
};

use crate::{
    db::Database,
    stores::{macro_store, micro_store, slash_store},
};

/// Thread-safe RocksDB-backed implementation of [`Persistence`].
#[derive(Debug, Clone)]
pub struct RocksPersistence {
    db: Arc<Database>,
    /// Monotonic counter for [`Persistence::append_slash_evidence`].
    seq: Arc<std::sync::atomic::AtomicU64>,
}

impl RocksPersistence {
    /// Wrap an already-opened [`Database`].
    #[must_use]
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            db,
            seq: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Borrow the underlying database (for tests + admin code).
    #[must_use]
    pub fn database(&self) -> &Arc<Database> {
        &self.db
    }
}

impl Persistence for RocksPersistence {
    fn store_micro_qc(&self, qc: &MicroQc) -> consensus::Result<()> {
        micro_store::put_qc(&self.db, qc).map_err(|e| map_err(&e))
    }

    fn store_macro_checkpoint(&self, cp: &MacroCheckpoint) -> consensus::Result<()> {
        macro_store::put_checkpoint(&self.db, cp).map_err(|e| map_err(&e))
    }

    fn store_macro_qc(&self, qc: &MacroQc) -> consensus::Result<()> {
        macro_store::put_qc(&self.db, qc).map_err(|e| map_err(&e))
    }

    fn append_slash_evidence(&self, ev: &SlashEvidence) -> consensus::Result<()> {
        let next = self.seq.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        slash_store::append(&self.db, next, ev).map_err(|e| map_err(&e))
    }

    fn macro_checkpoint_at(&self, height: Height) -> consensus::Result<Option<MacroCheckpoint>> {
        macro_store::get_checkpoint(&self.db, height).map_err(|e| map_err(&e))
    }

    fn macro_qc_for(&self, checkpoint_hash: &Hash32) -> consensus::Result<Option<MacroQc>> {
        macro_store::get_qc(&self.db, checkpoint_hash).map_err(|e| map_err(&e))
    }
}

fn map_err(e: &crate::Error) -> consensus::Error {
    consensus::Error::Persistence(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::StorageConfig, db::Database};
    use tempfile::tempdir;
    use types::{
        crypto_types::{BlsAggSig, BlsSig, Hash32},
        macros::{AggregationMode, MacroCheckpoint, MacroQc},
        primitives::{Epoch, Height},
    };

    fn fresh_db() -> Arc<Database> {
        let dir = tempdir().unwrap();
        let cfg = StorageConfig {
            path: dir.path().to_path_buf(),
            create_if_missing: true,
            max_total_wal_size_mb: 16,
        };
        Arc::new(Database::open(&cfg).unwrap())
    }

    #[test]
    fn store_and_fetch_macro_checkpoint_via_trait() {
        let db = fresh_db();
        let p = RocksPersistence::new(db);
        let cp = MacroCheckpoint {
            height: Height(7),
            epoch: Epoch(1),
            parent: Hash32::zero(),
            micro_root: Hash32([1; 32]),
            hash: Hash32([2; 32]),
        };
        p.store_macro_checkpoint(&cp).unwrap();
        let got = p.macro_checkpoint_at(Height(7)).unwrap();
        assert_eq!(got, Some(cp));
    }

    #[test]
    fn store_and_fetch_macro_qc_via_trait() {
        let db = fresh_db();
        let p = RocksPersistence::new(db);
        let qc = MacroQc {
            checkpoint_hash: Hash32([3; 32]),
            mode: AggregationMode::Mode0Flat,
            agg: BlsAggSig {
                sig: BlsSig([0; 96]),
                bitmap: vec![0xFF],
            },
        };
        p.store_macro_qc(&qc).unwrap();
        let got = p.macro_qc_for(&Hash32([3; 32])).unwrap();
        assert_eq!(got, Some(qc));
    }
}
