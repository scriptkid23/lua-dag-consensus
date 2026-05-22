//! Casper-FFG surround-vote detector.

use types::{primitives::ValidatorId, slashing::SurroundVote};

use crate::{error::Result, macro_fin::vote_book::VoteBook};

/// Casper rule: `a` surrounds `b` when `a.source < b.source <= b.target < a.target`.
fn surrounds(a_source: u64, a_target: u64, b_source: u64, b_target: u64) -> bool {
    a_source < b_source && b_source <= b_target && b_target < a_target
}

/// Scan `book` for a surround vote committed by `validator`.
pub fn scan_for_surround(
    book: &VoteBook,
    validator: &ValidatorId,
) -> Result<Option<SurroundVote>> {
    let votes = book.votes_of(validator);
    if votes.len() < 2 {
        return Ok(None);
    }
    let new = votes[votes.len() - 1];
    for old in &votes[..votes.len() - 1] {
        if surrounds(
            new.source.0,
            new.target.0,
            old.source.0,
            old.target.0,
        ) {
            return Ok(Some(SurroundVote {
                validator: *validator,
                a_source: new.source,
                a_target: new.target,
                a_sig: new.sig,
                a_checkpoint: new.checkpoint,
                b_source: old.source,
                b_target: old.target,
                b_sig: old.sig,
                b_checkpoint: old.checkpoint,
            }));
        }
        if surrounds(
            old.source.0,
            old.target.0,
            new.source.0,
            new.target.0,
        ) {
            return Ok(Some(SurroundVote {
                validator: *validator,
                a_source: old.source,
                a_target: old.target,
                a_sig: old.sig,
                a_checkpoint: old.checkpoint,
                b_source: new.source,
                b_target: new.target,
                b_sig: new.sig,
                b_checkpoint: new.checkpoint,
            }));
        }
    }
    Ok(None)
}

/// Verify surround-vote evidence signatures.
pub fn verify(ev: &SurroundVote, set: &types::validator::ValidatorSet) -> Result<()> {
    use crypto::{bls::PublicKey, hash::dst, bls::sign::verify};
    use crate::macro_fin::{messages, vote_book::VoteRecord};

    if !surrounds(
        ev.a_source.0,
        ev.a_target.0,
        ev.b_source.0,
        ev.b_target.0,
    ) {
        return Err(crate::Error::InvalidConfig("not a surround relation".into()));
    }
    let entry = set
        .entries
        .iter()
        .find(|e| e.id == ev.validator)
        .ok_or_else(|| crate::Error::InvalidConfig("unknown validator".into()))?;
    let pk = PublicKey::from_bytes(&entry.bls_pubkey)
        .map_err(|_| crate::Error::InvalidConfig("invalid bls pubkey".into()))?;

    let outer = VoteRecord {
        source: ev.a_source,
        target: ev.a_target,
        checkpoint: ev.a_checkpoint,
        sig: ev.a_sig,
    };
    let inner = VoteRecord {
        source: ev.b_source,
        target: ev.b_target,
        checkpoint: ev.b_checkpoint,
        sig: ev.b_sig,
    };
    for record in [outer, inner] {
        let msg = messages::vote_message(&record);
        verify(&pk, dst::MACRO_VOTE, &msg, &record.sig)
            .map_err(|_| crate::Error::InvalidConfig("invalid vote sig".into()))?;
    }
    Ok(())
}
