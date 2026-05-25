//! Read-only consensus queries over RocksDB (plan 06b-l3).

use std::sync::Arc;

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

use crate::live_dag::LiveDag;

/// `ConsensusQuery` backed by [`RocksPersistence`] and optional live L1 index.
#[derive(Debug, Clone)]
pub struct RocksConsensusQuery {
    persistence: RocksPersistence,
    live_dag: Arc<LiveDag>,
}

impl RocksConsensusQuery {
    /// Wrap persistence plus the live L1 DAG index.
    #[must_use]
    pub fn new(persistence: RocksPersistence, live_dag: Arc<LiveDag>) -> Self {
        Self {
            persistence,
            live_dag,
        }
    }

    /// Macro checkpoint at `height`, if persisted.
    pub fn macro_checkpoint_at(&self, height: Height) -> Result<Option<MacroCheckpoint>> {
        self.persistence.macro_checkpoint_at(height)
    }

    /// Certified vertex hashes for rounds `from..=to` (07c causal-set RPC).
    pub fn causal_set(&self, from: Round, to: Round) -> Vec<Hash32> {
        self.live_dag.certified_hashes_in_range(from, to)
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

    fn blob_status(&self, blob: &BlobId) -> Result<BlobStatus> {
        self.persistence.blob_status(blob)
    }

    fn macro_checkpoint_hash(&self, height: Height) -> Result<Option<Hash32>> {
        Ok(self
            .persistence
            .macro_checkpoint_at(height)?
            .map(|cp| cp.hash))
    }
}
