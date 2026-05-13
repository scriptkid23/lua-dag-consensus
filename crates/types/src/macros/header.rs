//! Light-client header (forward-compat, spec §10).

use borsh::{BorshDeserialize, BorshSerialize};

use super::qc::MacroQc;
use crate::{
    crypto_types::Hash32,
    primitives::{Epoch, Height},
};

/// Compact header that a light client / sync-committee verifier consumes.
///
/// Not used by consensus today; defined now to lock in the wire shape.
/// `Serialize`/`Deserialize` are not derived because `MacroQc` carries
/// wire-only BLS material.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct MacroHeader {
    /// `MacroCheckpoint` height.
    pub height: Height,
    /// Validator-set epoch.
    pub epoch: Epoch,
    /// Hash of the parent `MacroHeader`.
    pub parent: Hash32,
    /// Hash of the underlying `MacroCheckpoint`.
    pub checkpoint_hash: Hash32,
    /// Macro QC over the checkpoint.
    pub qc: MacroQc,
}
