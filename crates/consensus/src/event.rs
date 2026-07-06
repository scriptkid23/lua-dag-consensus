//! `Event` — every input the state machine accepts.
//!
//! Sources:
//! - `net` adapter translates wire messages into `Event`s.
//! - `node::timer` emits `Event::TimerFired`.
//! - `node::validator_set_loader` emits `Event::ValidatorSetUpdated`.
//! - `sim::virtual_*` emits the full set for deterministic replay.

use borsh::{BorshDeserialize, BorshSerialize};
use types::{
    dag::{CertifiedVertex, VertexPartial, VertexProposal},
    macros::{MacroProposal, MacroQc},
    micro::MicroQc,
    primitives::{Epoch, ValidatorId},
    slashing::SlashEvidence,
    validator::ValidatorSet,
};

use types::crypto_types::{BlsAggSig, BlsSig, Hash32};

/// Subnet index used by Mode A aggregation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, BorshSerialize, BorshDeserialize)]
pub struct SubnetId(pub u32);

/// Opaque timer identifier (allocated by `leader::timeout`).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, BorshSerialize, BorshDeserialize)]
pub struct TimerId(pub u64);

/// Partial BLS signature contribution from a single validator on a
/// subnet (Mode A) or globally (Mode 0).
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct BlsPartial {
    /// Subnet identifier (`0` for Mode 0 flat).
    pub subnet: SubnetId,
    /// Validator signing.
    pub validator: ValidatorId,
    /// Hash of the checkpoint being attested.
    pub checkpoint_hash: Hash32,
    /// Partial signature.
    pub sig: BlsSig,
}

/// Subnet-level aggregate produced by a subnet aggregator.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct SubnetAggregate {
    /// Subnet identifier.
    pub subnet: SubnetId,
    /// Hash of the checkpoint being attested.
    pub checkpoint_hash: Hash32,
    /// Aggregated BLS signature.
    pub agg: BlsAggSig,
}

/// All inputs to the consensus state machine.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub enum Event {
    /// A new certified vertex arrived from L1.
    CertifiedVertexReceived(CertifiedVertex),
    /// A MicroQc was assembled locally (after we crossed ⅔ stake).
    MicroQcAssembled(MicroQc),
    /// A macro proposal was received from a proposer.
    MacroProposalReceived(MacroProposal),
    /// A partial BLS signature was received.
    BlsPartialReceived(BlsPartial),
    /// A subnet aggregate was received (Mode A).
    SubnetAggregateReceived(SubnetAggregate),
    /// A macro QC was received (used when joining late or for Mode B).
    MacroQcReceived(MacroQc),
    /// A scheduled timer fired.
    TimerFired(TimerId),
    /// The validator set rotated.
    ValidatorSetUpdated {
        /// Epoch the set is valid for.
        epoch: Epoch,
        /// The new set.
        set: ValidatorSet,
    },
    /// Slashing evidence was observed.
    SlashEvidenceFound(SlashEvidence),
    /// A vertex proposal header arrived from a peer (L1 distributed cert).
    VertexProposalReceived(VertexProposal),
    /// A vertex partial vote arrived (routed to the proposal's author).
    VertexPartialReceived(VertexPartial),
}

#[cfg(test)]
mod tests {
    use super::*;
    use borsh::to_vec;
    use crypto::bls::Bitmap;

    #[test]
    fn event_round_trips() {
        let ev = Event::TimerFired(TimerId(7));
        let bytes = to_vec(&ev).unwrap();
        let ev2: Event = borsh::from_slice(&bytes).unwrap();
        assert_eq!(ev, ev2);
    }

    #[test]
    fn _ensure_bitmap_link_exists() {
        let _b = Bitmap::new(8);
    }
}
