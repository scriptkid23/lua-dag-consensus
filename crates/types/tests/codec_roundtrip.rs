//! End-to-end Borsh round-trip for every public type.
//!
//! If a new wire type is added to `crates/types/`, add a case here.

use types::{
    crypto_types::{BlsAggSig, BlsPubkey, BlsSig, Hash32, Pop, VrfProof},
    dag::{BlobRef, CertifiedVertex, ChunkRef, Vertex},
    macros::{AggregationMode, MacroCheckpoint, MacroHeader, MacroProposal, MacroQc},
    micro::{MicroCheckpoint, MicroQc},
    primitives::{BlobId, Epoch, Height, Round, StakeWeight, ValidatorId},
    slashing::{DoubleVote, MacroEquivocation, SlashEvidence, SurroundVote},
    validator::{DkgCommitment, ValidatorEntry, ValidatorIdentity, ValidatorSet},
};

fn round_trip<T>(value: &T)
where
    T: borsh::BorshSerialize + borsh::BorshDeserialize + PartialEq + std::fmt::Debug,
{
    let bytes = borsh::to_vec(value).expect("serialize");
    let back: T = borsh::from_slice(&bytes).expect("deserialize");
    assert_eq!(value, &back, "round-trip mismatch");
}

#[test]
fn primitives_round_trip() {
    round_trip(&Round(1));
    round_trip(&Height(2));
    round_trip(&Epoch(3));
    round_trip(&StakeWeight(4));
    round_trip(&ValidatorId([5; 32]));
    round_trip(&BlobId([6; 32]));
}

#[test]
fn crypto_types_round_trip() {
    round_trip(&BlsPubkey([1; 48]));
    round_trip(&BlsSig([2; 96]));
    round_trip(&Pop(BlsSig([3; 96])));
    round_trip(&VrfProof([4; 80]));
    round_trip(&Hash32([5; 32]));
    round_trip(&BlsAggSig {
        sig: BlsSig([6; 96]),
        bitmap: vec![0xAA, 0xBB],
    });
}

#[test]
fn dag_types_round_trip() {
    let v = Vertex {
        round: Round(1),
        author: ValidatorId([1; 32]),
        parents: vec![Hash32([2; 32])],
        blobs: vec![BlobRef {
            blob_id: BlobId([3; 32]),
            commitment: Hash32([4; 32]),
            size_bytes: 1024,
        }],
        hash: Hash32([5; 32]),
    };
    round_trip(&v);
    round_trip(&CertifiedVertex {
        vertex: v,
        certificate: BlsAggSig {
            sig: BlsSig([6; 96]),
            bitmap: vec![0xFF],
        },
    });
    round_trip(&ChunkRef {
        blob_id: BlobId([7; 32]),
        index: 9,
    });
}

#[test]
fn micro_types_round_trip() {
    round_trip(&MicroCheckpoint {
        anchor_round: Round(1),
        anchor_author: ValidatorId([1; 32]),
        anchor_hash: Hash32([2; 32]),
        linearized: vec![Hash32([3; 32])],
        hash: Hash32([4; 32]),
    });
    round_trip(&MicroQc {
        checkpoint_hash: Hash32([5; 32]),
        agg: BlsAggSig {
            sig: BlsSig([6; 96]),
            bitmap: vec![0x0F],
        },
    });
}

#[test]
fn macro_types_round_trip() {
    let cp = MacroCheckpoint {
        height: Height(1),
        epoch: Epoch(2),
        parent: Hash32([1; 32]),
        micro_root: Hash32([2; 32]),
        hash: Hash32([3; 32]),
    };
    round_trip(&cp);
    let qc = MacroQc {
        checkpoint_hash: cp.hash,
        mode: AggregationMode::ModeASubnet,
        agg: BlsAggSig {
            sig: BlsSig([7; 96]),
            bitmap: vec![0xCC],
        },
    };
    round_trip(&qc);
    round_trip(&MacroHeader {
        height: cp.height,
        epoch: cp.epoch,
        parent: cp.parent,
        checkpoint_hash: cp.hash,
        qc,
    });
    round_trip(&MacroProposal {
        checkpoint: cp,
        proposer: ValidatorId([8; 32]),
        vrf_proof: VrfProof([9; 80]),
        proposer_sig: BlsSig([10; 96]),
    });
}

#[test]
fn validator_types_round_trip() {
    let entry = ValidatorEntry {
        id: ValidatorId([1; 32]),
        bls_pubkey: BlsPubkey([2; 48]),
        stake: StakeWeight(1000),
        identity: ValidatorIdentity {
            asn: Some(13335),
            cloud: Some("aws".into()),
            region: Some("eu-west-1".into()),
        },
    };
    round_trip(&entry);
    round_trip(&ValidatorSet {
        epoch: Epoch(1),
        entries: vec![entry],
        total_stake: StakeWeight(1000),
    });
    round_trip(&DkgCommitment {
        validator: ValidatorId([3; 32]),
        epoch: Epoch(1),
        bls_pubkey: BlsPubkey([4; 48]),
        shares_root: Hash32([5; 32]),
    });
}

#[test]
fn slashing_evidence_round_trip() {
    let cp = MacroCheckpoint {
        height: Height(1),
        epoch: Epoch(1),
        parent: Hash32::zero(),
        micro_root: Hash32::zero(),
        hash: Hash32::zero(),
    };
    round_trip(&SlashEvidence::MacroEquivocation(MacroEquivocation {
        validator: ValidatorId([1; 32]),
        a: (cp.clone(), BlsSig([0; 96])),
        b: (cp, BlsSig([1; 96])),
    }));
    round_trip(&SlashEvidence::Surround(SurroundVote {
        validator: ValidatorId([1; 32]),
        a_source: Epoch(1),
        a_target: Epoch(10),
        a_sig: BlsSig([0; 96]),
        b_source: Epoch(3),
        b_target: Epoch(8),
        b_sig: BlsSig([0; 96]),
    }));
    round_trip(&SlashEvidence::DoubleVote(DoubleVote {
        validator: ValidatorId([1; 32]),
        target: Epoch(5),
        a_sig: BlsSig([0; 96]),
        b_sig: BlsSig([1; 96]),
    }));
}
