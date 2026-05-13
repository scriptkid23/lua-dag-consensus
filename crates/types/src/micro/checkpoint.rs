//! `MicroCheckpoint`: result of a Bullshark wave commit.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{
    crypto_types::Hash32,
    primitives::{Round, ValidatorId},
};

/// A canonical, linearized batch of certified vertices anchored at one wave.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct MicroCheckpoint {
    /// Round of the anchor vertex.
    pub anchor_round: Round,
    /// Author of the anchor vertex.
    pub anchor_author: ValidatorId,
    /// Hash of the anchor vertex.
    pub anchor_hash: Hash32,
    /// Hashes of all vertices in the linearized closure, in commit order.
    pub linearized: Vec<Hash32>,
    /// Deterministic hash of this checkpoint (computed by canonical codec).
    pub hash: Hash32,
}
