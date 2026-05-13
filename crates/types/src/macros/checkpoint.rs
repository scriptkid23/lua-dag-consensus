//! `MacroCheckpoint`: the L3 commitment, published every W micro-slots.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{
    crypto_types::Hash32,
    primitives::{Epoch, Height},
};

/// A `MacroCheckpoint` summarises W consecutive `MicroCheckpoint`s.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct MacroCheckpoint {
    /// Height (monotonic; one per macro window).
    pub height: Height,
    /// Validator-set epoch.
    pub epoch: Epoch,
    /// Hash of the previous `MacroCheckpoint` (or `Hash32::zero()` for genesis).
    pub parent: Hash32,
    /// Root of the W micro-checkpoints covered by this window (Merkle/sequence root).
    pub micro_root: Hash32,
    /// Deterministic hash of this `MacroCheckpoint`.
    pub hash: Hash32,
}
