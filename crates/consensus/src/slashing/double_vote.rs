//! Casper-FFG double-vote detector.

use types::{primitives::ValidatorId, slashing::DoubleVote};

use crate::{error::Result, macro_fin::vote_book::VoteBook};

/// Scan for two votes at the same target epoch with different checkpoints.
pub fn scan_for_double_vote(
    book: &VoteBook,
    validator: &ValidatorId,
) -> Result<Option<DoubleVote>> {
    let votes = book.votes_of(validator);
    if votes.len() < 2 {
        return Ok(None);
    }
    let new = votes[votes.len() - 1];
    for old in &votes[..votes.len() - 1] {
        if old.target == new.target && old.checkpoint != new.checkpoint {
            return Ok(Some(DoubleVote {
                validator: *validator,
                target: new.target,
                a_checkpoint: old.checkpoint,
                a_sig: old.sig,
                b_checkpoint: new.checkpoint,
                b_sig: new.sig,
            }));
        }
    }
    Ok(None)
}

/// Verify double-vote evidence signatures.
pub fn verify(ev: &DoubleVote, set: &types::validator::ValidatorSet) -> Result<()> {
    use crypto::{bls::PublicKey, hash::dst, bls::sign::verify};
    use crate::macro_fin::{messages, vote_book::VoteRecord};

    if ev.a_checkpoint == ev.b_checkpoint {
        return Err(crate::Error::InvalidConfig(
            "double vote requires distinct checkpoints".into(),
        ));
    }
    let entry = set
        .entries
        .iter()
        .find(|e| e.id == ev.validator)
        .ok_or_else(|| crate::Error::InvalidConfig("unknown validator".into()))?;
    let pk = PublicKey::from_bytes(&entry.bls_pubkey)
        .map_err(|_| crate::Error::InvalidConfig("invalid bls pubkey".into()))?;

    let votes = [
        VoteRecord {
            source: types::primitives::Epoch(ev.target.0.saturating_sub(1)),
            target: ev.target,
            checkpoint: ev.a_checkpoint,
            sig: ev.a_sig,
        },
        VoteRecord {
            source: types::primitives::Epoch(ev.target.0.saturating_sub(1)),
            target: ev.target,
            checkpoint: ev.b_checkpoint,
            sig: ev.b_sig,
        },
    ];
    for record in votes {
        let msg = messages::vote_message(&record);
        verify(&pk, dst::MACRO_VOTE, &msg, &record.sig)
            .map_err(|_| crate::Error::InvalidConfig("invalid vote sig".into()))?;
    }
    Ok(())
}
