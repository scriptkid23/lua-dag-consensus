//! L3 gossip wire roundtrip tests (plan 06b-l3).

use consensus::action::Action;
use consensus::event::{BlsPartial, Event, SubnetAggregate, SubnetId};
use net::gossip_wire::{inbound_message, outbound_broadcast};
use types::crypto_types::{BlsAggSig, BlsSig, Hash32, VrfProof};
use types::macros::{AggregationMode, MacroCheckpoint, MacroProposal, MacroQc};
use types::primitives::{Epoch, Height, ValidatorId};
use types::slashing::{DoubleVote, SlashEvidence};

fn macro_proposal_fixture() -> MacroProposal {
    MacroProposal {
        checkpoint: MacroCheckpoint {
            height: Height(1),
            epoch: Epoch(0),
            parent: Hash32::zero(),
            micro_root: Hash32([1; 32]),
            hash: Hash32([2; 32]),
        },
        proposer: ValidatorId([3; 32]),
        vrf_proof: VrfProof([4; 80]),
        proposer_sig: BlsSig([5; 96]),
    }
}

#[test]
fn macro_proposal_roundtrips_on_wire() {
    let proposal = macro_proposal_fixture();
    let action = Action::BroadcastMacroProposal(proposal.clone());
    let (topic, bytes) = outbound_broadcast(&action).unwrap().unwrap();
    let ev = inbound_message(&topic.ident().to_string(), &bytes)
        .unwrap()
        .unwrap();
    assert!(matches!(ev, Event::MacroProposalReceived(p) if p == proposal));
}

#[test]
fn macro_qc_roundtrips_on_wire() {
    let qc = MacroQc {
        checkpoint_hash: Hash32([8; 32]),
        mode: AggregationMode::Mode0Flat,
        agg: BlsAggSig {
            sig: BlsSig([0; 96]),
            bitmap: vec![0xFF],
        },
    };
    let action = Action::BroadcastMacroQc(qc.clone());
    let (topic, bytes) = outbound_broadcast(&action).unwrap().unwrap();
    let ev = inbound_message(&topic.ident().to_string(), &bytes)
        .unwrap()
        .unwrap();
    assert!(matches!(ev, Event::MacroQcReceived(q) if q == qc));
}

#[test]
fn slash_evidence_roundtrips_on_wire() {
    let evidence = SlashEvidence::DoubleVote(DoubleVote {
        validator: ValidatorId([9; 32]),
        target: Epoch(0),
        a_checkpoint: Hash32([10; 32]),
        a_sig: BlsSig([0; 96]),
        b_checkpoint: Hash32([11; 32]),
        b_sig: BlsSig([1; 96]),
    });
    let action = Action::EmitSlashEvidence {
        offender: ValidatorId([9; 32]),
        evidence: evidence.clone(),
    };
    let (topic, bytes) = outbound_broadcast(&action).unwrap().unwrap();
    let ev = inbound_message(&topic.ident().to_string(), &bytes)
        .unwrap()
        .unwrap();
    assert!(matches!(ev, Event::SlashEvidenceFound(e) if e == evidence));
}

#[test]
fn subnet_aggregate_roundtrips_on_wire() {
    let agg = SubnetAggregate {
        subnet: SubnetId(2),
        checkpoint_hash: Hash32([11; 32]),
        agg: BlsAggSig {
            sig: BlsSig([0; 96]),
            bitmap: vec![0x0F],
        },
    };
    let action = Action::BroadcastSubnetAggregate(agg.clone());
    let (topic, bytes) = outbound_broadcast(&action).unwrap().unwrap();
    let ev = inbound_message(&topic.ident().to_string(), &bytes)
        .unwrap()
        .unwrap();
    assert!(matches!(ev, Event::SubnetAggregateReceived(a) if a == agg));
}

#[test]
fn bls_partial_roundtrips_on_wire() {
    let partial = BlsPartial {
        subnet: SubnetId(3),
        validator: ValidatorId([12; 32]),
        checkpoint_hash: Hash32([13; 32]),
        sig: BlsSig([0; 96]),
    };
    let action = Action::BroadcastBlsPartial(partial.clone());
    let (topic, bytes) = outbound_broadcast(&action).unwrap().unwrap();
    let ev = inbound_message(&topic.ident().to_string(), &bytes)
        .unwrap()
        .unwrap();
    assert!(matches!(ev, Event::BlsPartialReceived(p) if p == partial));
}
