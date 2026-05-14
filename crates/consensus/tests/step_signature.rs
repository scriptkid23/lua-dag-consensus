//! Smoke test: `StateMachine::step` accepts every `Event` variant and
//! returns an empty action list under the skeleton implementation.

use consensus::{
    Config, StateMachine,
    event::{BlsPartial, Event, SubnetAggregate, SubnetId, TimerId},
};
use types::{
    crypto_types::{BlsAggSig, BlsSig, Hash32, VrfProof},
    dag::{CertifiedVertex, Vertex},
    macros::{AggregationMode, MacroCheckpoint, MacroProposal, MacroQc},
    micro::MicroQc,
    primitives::{Epoch, Height, Round, StakeWeight, ValidatorId},
    slashing::{DoubleVote, SlashEvidence},
    validator::ValidatorSet,
};

fn fixture_certified() -> CertifiedVertex {
    CertifiedVertex {
        vertex: Vertex {
            round: Round(0),
            author: ValidatorId([0; 32]),
            parents: vec![],
            blobs: vec![],
            hash: Hash32::zero(),
        },
        certificate: BlsAggSig {
            sig: BlsSig([0; 96]),
            bitmap: vec![],
        },
    }
}

fn fixture_macro_checkpoint() -> MacroCheckpoint {
    MacroCheckpoint {
        height: Height(0),
        epoch: Epoch(0),
        parent: Hash32::zero(),
        micro_root: Hash32::zero(),
        hash: Hash32::zero(),
    }
}

fn fixture_macro_qc() -> MacroQc {
    MacroQc {
        checkpoint_hash: Hash32::zero(),
        mode: AggregationMode::Mode0Flat,
        agg: BlsAggSig {
            sig: BlsSig([0; 96]),
            bitmap: vec![],
        },
    }
}

#[test]
fn step_returns_empty_for_every_variant() {
    let mut sm = StateMachine::new(Config::default_table_17_1());
    let events = [
        Event::CertifiedVertexReceived(fixture_certified()),
        Event::MicroQcAssembled(MicroQc {
            checkpoint_hash: Hash32::zero(),
            agg: BlsAggSig {
                sig: BlsSig([0; 96]),
                bitmap: vec![],
            },
        }),
        Event::MacroProposalReceived(MacroProposal {
            checkpoint: fixture_macro_checkpoint(),
            proposer: ValidatorId([0; 32]),
            vrf_proof: VrfProof([0; 80]),
            proposer_sig: BlsSig([0; 96]),
        }),
        Event::BlsPartialReceived(BlsPartial {
            subnet: SubnetId(0),
            validator: ValidatorId([0; 32]),
            checkpoint_hash: Hash32::zero(),
            sig: BlsSig([0; 96]),
        }),
        Event::SubnetAggregateReceived(SubnetAggregate {
            subnet: SubnetId(0),
            checkpoint_hash: Hash32::zero(),
            agg: BlsAggSig {
                sig: BlsSig([0; 96]),
                bitmap: vec![],
            },
        }),
        Event::MacroQcReceived(fixture_macro_qc()),
        Event::TimerFired(TimerId(0)),
        Event::ValidatorSetUpdated {
            epoch: Epoch(0),
            set: ValidatorSet {
                epoch: Epoch(0),
                entries: vec![],
                total_stake: StakeWeight(0),
            },
        },
        Event::SlashEvidenceFound(SlashEvidence::DoubleVote(DoubleVote {
            validator: ValidatorId([0; 32]),
            target: Epoch(0),
            a_sig: BlsSig([0; 96]),
            b_sig: BlsSig([1; 96]),
        })),
    ];
    for ev in events {
        let actions = sm.step(ev).expect("step never errors in skeleton");
        assert!(actions.is_empty(), "skeleton must emit zero actions");
    }
}
