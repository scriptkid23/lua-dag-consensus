//! `MacroQc`: aggregated BLS attestation over a `MacroCheckpoint`.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::crypto_types::{BlsAggSig, Hash32};

/// Adaptive aggregation mode (whitepaper §9.2, Eq. 9.1/9.2).
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum AggregationMode {
    /// Flat aggregation, `Ne < 500`.
    Mode0Flat = 0,
    /// Subnet aggregation with rotation, `Ne ≥ 500`.
    ModeASubnet = 1,
    /// Leaderless fallback (proposer failed primary + backup slots).
    ModeBLeaderless = 2,
}

/// Quorum certificate over a `MacroCheckpoint`.
///
/// `Serialize`/`Deserialize` are not derived because `BlsAggSig` carries
/// raw 96-byte BLS material that is wire-only (Borsh).
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct MacroQc {
    /// Hash of the checkpoint being attested.
    pub checkpoint_hash: Hash32,
    /// Aggregation mode that produced this QC.
    pub mode: AggregationMode,
    /// Aggregate BLS signature plus signer bitmap.
    pub agg: BlsAggSig,
}
