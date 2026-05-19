//! `ValidatorSet` snapshot, epoch-indexed.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use super::identity::ValidatorIdentity;
use crate::{
    crypto_types::BlsPubkey,
    primitives::{Epoch, StakeWeight, ValidatorId},
};

/// A single validator entry in the active set.
///
/// `Borsh` is the canonical wire encoding. `serde` is included for config
/// and bootstrap file formats (e.g. TOML validator sets).
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
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
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct ValidatorSet {
    /// Epoch this snapshot is valid for.
    pub epoch: Epoch,
    /// Entries sorted by `ValidatorId` (deterministic).
    pub entries: Vec<ValidatorEntry>,
    /// Sum of `entries[*].stake`.
    pub total_stake: StakeWeight,
}
