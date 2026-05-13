//! `MacroProposal`: proposer → committee message containing a candidate
//! `MacroCheckpoint` plus the proposer's signature.

use borsh::{BorshDeserialize, BorshSerialize};

use super::checkpoint::MacroCheckpoint;
use crate::{
    crypto_types::{BlsSig, VrfProof},
    primitives::ValidatorId,
};

/// A macro-window proposal.
///
/// `Serialize`/`Deserialize` are not derived because `BlsSig` and
/// `VrfProof` are wire-only (Borsh).
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct MacroProposal {
    /// Candidate checkpoint.
    pub checkpoint: MacroCheckpoint,
    /// Proposer.
    pub proposer: ValidatorId,
    /// VRF proof binding proposer to this slot's beacon.
    pub vrf_proof: VrfProof,
    /// Proposer signature over `checkpoint.hash`.
    pub proposer_sig: BlsSig,
}
