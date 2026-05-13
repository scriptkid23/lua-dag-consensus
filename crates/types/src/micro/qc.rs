//! `MicroQc`: aggregated micro-vote certificate (≥ ⌈2/3·C⌉).

use borsh::{BorshDeserialize, BorshSerialize};

use crate::crypto_types::{BlsAggSig, Hash32};

/// Quorum certificate over a `MicroCheckpoint`.
///
/// `Serialize`/`Deserialize` are not derived because `BlsAggSig` is
/// wire-only (Borsh).
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct MicroQc {
    /// Hash of the checkpoint being certified.
    pub checkpoint_hash: Hash32,
    /// Aggregate BLS signature plus signer bitmap.
    pub agg: BlsAggSig,
}
