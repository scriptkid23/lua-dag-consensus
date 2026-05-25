//! Canonical byte encodings for L3 BLS sign/verify.

use borsh::BorshSerialize;
use types::{
    crypto_types::Hash32,
    macros::MacroCheckpoint,
    primitives::{Epoch, ValidatorId},
};

use crate::macro_fin::vote_book::VoteRecord;

/// Partial signature message: `validator_id ‖ checkpoint_hash`.
#[must_use]
pub fn partial_message(validator: &ValidatorId, checkpoint_hash: &Hash32) -> Vec<u8> {
    let mut m = Vec::with_capacity(64);
    m.extend_from_slice(validator.as_bytes());
    m.extend_from_slice(&checkpoint_hash.0);
    m
}

/// Macro proposer signature message: `proposer ‖ checkpoint.hash`.
#[must_use]
pub fn proposer_message(proposer: &ValidatorId, checkpoint: &MacroCheckpoint) -> Vec<u8> {
    let mut m = Vec::with_capacity(64);
    m.extend_from_slice(proposer.as_bytes());
    m.extend_from_slice(&checkpoint.hash.0);
    m
}

/// Macro QC body: canonical Borsh bytes of the checkpoint.
#[must_use]
pub fn checkpoint_message(cp: &MacroCheckpoint) -> Vec<u8> {
    borsh::to_vec(cp).expect("MacroCheckpoint must borsh-encode")
}

/// Macro vote signing payload (Casper-FFG fields only).
#[must_use]
pub fn vote_message(record: &VoteRecord) -> Vec<u8> {
    #[derive(BorshSerialize)]
    struct VotePayload {
        source: Epoch,
        target: Epoch,
        checkpoint: Hash32,
    }
    borsh::to_vec(&VotePayload {
        source: record.source,
        target: record.target,
        checkpoint: record.checkpoint,
    })
    .expect("vote payload must borsh-encode")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_message_is_stable() {
        let v = ValidatorId([1; 32]);
        let h = Hash32([2; 32]);
        assert_eq!(partial_message(&v, &h), partial_message(&v, &h));
    }

    #[test]
    fn proposer_message_matches_partial_when_same_id_and_hash() {
        let v = ValidatorId([1; 32]);
        let cp = MacroCheckpoint {
            height: types::primitives::Height(0),
            epoch: Epoch(0),
            parent: Hash32([0x99; 32]),
            micro_root: Hash32([3; 32]),
            hash: Hash32([2; 32]),
        };
        assert_eq!(
            partial_message(&v, &cp.hash),
            proposer_message(&v, &cp)
        );
    }
}
