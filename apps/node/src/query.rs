//! Read-only consensus queries over RocksDB (plan 06b-l3).

use consensus::{
    Result,
    api::{query::ConsensusQuery, tier::BlobStatus},
    ports::Persistence,
};
use storage::RocksPersistence;
use types::{
    crypto_types::Hash32,
    macros::{MacroCheckpoint, MacroQc},
    primitives::{BlobId, Height, Round},
};

/// `ConsensusQuery` backed by [`RocksPersistence`].
#[derive(Debug, Clone)]
pub struct RocksConsensusQuery {
    persistence: RocksPersistence,
}

impl RocksConsensusQuery {
    /// Wrap an open persistence handle.
    #[must_use]
    pub fn new(persistence: RocksPersistence) -> Self {
        Self { persistence }
    }

    /// Macro checkpoint at `height`, if persisted.
    pub fn macro_checkpoint_at(&self, height: Height) -> Result<Option<MacroCheckpoint>> {
        self.persistence.macro_checkpoint_at(height)
    }
}

impl ConsensusQuery for RocksConsensusQuery {
    fn latest_finalized(&self) -> Result<Option<MacroQc>> {
        let mut best: Option<(Height, MacroQc)> = None;
        for h in 0..128u64 {
            let height = Height(h);
            let Some(cp) = self.persistence.macro_checkpoint_at(height)? else {
                continue;
            };
            let Some(qc) = self.persistence.macro_qc_for(&cp.hash)? else {
                continue;
            };
            if best.as_ref().is_none_or(|(bh, _)| height > *bh) {
                best = Some((height, qc));
            }
        }
        Ok(best.map(|(_, qc)| qc))
    }

    fn micro_head(&self) -> Result<Round> {
        Ok(Round(0))
    }

    fn blob_status(&self, _blob: &BlobId) -> Result<BlobStatus> {
        Ok(BlobStatus::Accepted)
    }

    fn macro_checkpoint_hash(&self, height: Height) -> Result<Option<Hash32>> {
        Ok(self
            .persistence
            .macro_checkpoint_at(height)?
            .map(|cp| cp.hash))
    }
}
