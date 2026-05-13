//! Slashing evidence variants. Verification logic lives in
//! `consensus::slashing`; this file is just the wire shape.
//!
//! `Serialize`/`Deserialize` are not derived: every variant carries a
//! `BlsSig` (or a `MacroCheckpoint` already nested in a tuple with one),
//! which is wire-only (Borsh).

use borsh::{BorshDeserialize, BorshSerialize};

use crate::{
    crypto_types::BlsSig,
    macros::checkpoint::MacroCheckpoint,
    primitives::{Epoch, ValidatorId},
};

/// Two macro proposals signed by the same proposer at the same height.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct MacroEquivocation {
    /// Offender.
    pub validator: ValidatorId,
    /// First conflicting checkpoint + signature.
    pub a: (MacroCheckpoint, BlsSig),
    /// Second conflicting checkpoint + signature.
    pub b: (MacroCheckpoint, BlsSig),
}

/// Casper-FFG surround-vote evidence: vote `a` surrounds vote `b`
/// (`a.source < b.source ≤ b.target < a.target`).
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct SurroundVote {
    /// Offender.
    pub validator: ValidatorId,
    /// Outer vote source epoch.
    pub a_source: Epoch,
    /// Outer vote target epoch.
    pub a_target: Epoch,
    /// Outer vote signature.
    pub a_sig: BlsSig,
    /// Inner vote source epoch.
    pub b_source: Epoch,
    /// Inner vote target epoch.
    pub b_target: Epoch,
    /// Inner vote signature.
    pub b_sig: BlsSig,
}

/// Two distinct votes at the same target epoch.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct DoubleVote {
    /// Offender.
    pub validator: ValidatorId,
    /// Common target epoch.
    pub target: Epoch,
    /// First vote signature.
    pub a_sig: BlsSig,
    /// Second vote signature.
    pub b_sig: BlsSig,
}

/// All slashing evidence variants.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub enum SlashEvidence {
    /// Equivocation on a macro proposal (100 %).
    MacroEquivocation(MacroEquivocation),
    /// Casper-FFG surround vote (50 %).
    Surround(SurroundVote),
    /// Double vote on the same target epoch (50 %).
    DoubleVote(DoubleVote),
}

#[cfg(test)]
mod tests {
    use super::*;
    use borsh::to_vec;

    use crate::{crypto_types::Hash32, primitives::Height};

    fn dummy_checkpoint() -> MacroCheckpoint {
        MacroCheckpoint {
            height: Height(0),
            epoch: Epoch(0),
            parent: Hash32::zero(),
            micro_root: Hash32::zero(),
            hash: Hash32::zero(),
        }
    }

    #[test]
    fn slash_evidence_round_trips_for_all_variants() {
        let ev = SlashEvidence::MacroEquivocation(MacroEquivocation {
            validator: ValidatorId([1; 32]),
            a: (dummy_checkpoint(), BlsSig([0; 96])),
            b: (dummy_checkpoint(), BlsSig([0; 96])),
        });
        let bytes = to_vec(&ev).unwrap();
        let ev2: SlashEvidence = borsh::from_slice(&bytes).unwrap();
        assert_eq!(ev, ev2);
    }
}
