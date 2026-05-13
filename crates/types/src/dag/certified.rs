//! L1 certified vertex (vertex + quorum certificate from the L1 layer).

use borsh::{BorshDeserialize, BorshSerialize};

use super::vertex::Vertex;
use crate::crypto_types::BlsAggSig;

/// A vertex carrying its L1 quorum certificate.
///
/// Consensus consumes this read-only via `consensus::ports::DagView`.
/// `Serialize`/`Deserialize` are not derived because `BlsAggSig` is
/// wire-only (Borsh).
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct CertifiedVertex {
    /// The vertex itself.
    pub vertex: Vertex,
    /// Aggregated certificate signature (validator bitmap inside).
    pub certificate: BlsAggSig,
}
