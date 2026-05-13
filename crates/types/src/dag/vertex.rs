//! L1 vertex (causal-set node).

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use super::refs::BlobRef;
use crate::{
    crypto_types::Hash32,
    primitives::{Round, ValidatorId},
};

/// A single uncertified DAG vertex.
///
/// In the spec this corresponds to a "Narwhal-class header" pre-certification.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct Vertex {
    /// Round in which the vertex was authored.
    pub round: Round,
    /// Author validator.
    pub author: ValidatorId,
    /// Parent vertex hashes (causal predecessors).
    pub parents: Vec<Hash32>,
    /// Blobs included by this vertex.
    pub blobs: Vec<BlobRef>,
    /// Deterministic hash; populated by the producer.
    pub hash: Hash32,
}
