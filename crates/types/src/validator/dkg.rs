//! DKG commitment skeleton. The full ceremony is out of scope this phase;
//! this struct exists so storage can persist commitments produced later.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::{
    crypto_types::{BlsPubkey, Hash32},
    primitives::{Epoch, ValidatorId},
};

/// A single validator's published DKG commitment for an epoch.
///
/// `Serialize`/`Deserialize` are not derived because `BlsPubkey` is
/// wire-only (Borsh).
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct DkgCommitment {
    /// Validator publishing the commitment.
    pub validator: ValidatorId,
    /// Epoch the commitment applies to.
    pub epoch: Epoch,
    /// BLS public key bound to this commitment.
    pub bls_pubkey: BlsPubkey,
    /// Hash over the encrypted shares fan-out.
    pub shares_root: Hash32,
}
