//! Per-validator vote history (epoch-indexed) for surround/double-vote detection.

use std::collections::HashMap;

use types::{
    crypto_types::{BlsSig, Hash32},
    primitives::{Epoch, ValidatorId},
};

/// A single macro vote: `(source, target, checkpoint_hash, sig)`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VoteRecord {
    /// Casper-FFG source epoch.
    pub source: Epoch,
    /// Casper-FFG target epoch.
    pub target: Epoch,
    /// Hash of the attested checkpoint.
    pub checkpoint: Hash32,
    /// BLS signature over [`crate::macro_fin::messages::vote_message`].
    pub sig: BlsSig,
}

/// Per-validator vote history.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VoteBook {
    /// Sorted by `target` per validator.
    votes: HashMap<ValidatorId, Vec<VoteRecord>>,
}

impl VoteBook {
    /// New empty book.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record `record` for `validator`.
    pub fn record(&mut self, validator: ValidatorId, record: VoteRecord) {
        self.votes.entry(validator).or_default().push(record);
    }

    /// Iterate `validator`'s votes (insertion order).
    #[must_use]
    pub fn votes_of(&self, validator: &ValidatorId) -> &[VoteRecord] {
        self.votes.get(validator).map_or(&[], Vec::as_slice)
    }
}
