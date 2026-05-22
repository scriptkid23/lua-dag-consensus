//! Pure-function verifier for `SlashEvidence`.

use types::{slashing::SlashEvidence, validator::ValidatorSet};

use crate::error::Result;

use super::{double_vote, equivocation, surround};

/// Verify a slashing evidence bundle against a validator set snapshot.
pub fn verify_evidence(ev: &SlashEvidence, set: &ValidatorSet) -> Result<()> {
    match ev {
        SlashEvidence::MacroEquivocation(e) => equivocation::verify(e, set),
        SlashEvidence::Surround(e) => surround::verify(e, set),
        SlashEvidence::DoubleVote(e) => double_vote::verify(e, set),
    }
}
