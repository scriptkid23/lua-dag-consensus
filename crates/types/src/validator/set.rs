//! `ValidatorSet` snapshot, epoch-indexed.

use borsh::{BorshDeserialize, BorshSerialize};

use super::identity::ValidatorIdentity;
use crate::{
    crypto_types::BlsPubkey,
    primitives::{Epoch, StakeWeight, ValidatorId},
};

/// A single validator entry in the active set.
///
/// `Serialize`/`Deserialize` are not derived because `BlsPubkey` is
/// wire-only (Borsh).
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct ValidatorEntry {
    /// Validator id.
    pub id: ValidatorId,
    /// BLS public key.
    pub bls_pubkey: BlsPubkey,
    /// Stake weight at the start of `epoch`.
    pub stake: StakeWeight,
    /// Optional diversity metadata.
    pub identity: ValidatorIdentity,
}

/// Validator-set snapshot for a specific epoch.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct ValidatorSet {
    /// Epoch this snapshot is valid for.
    pub epoch: Epoch,
    /// Entries sorted by `ValidatorId` (deterministic).
    pub entries: Vec<ValidatorEntry>,
    /// Sum of `entries[*].stake`.
    pub total_stake: StakeWeight,
}
