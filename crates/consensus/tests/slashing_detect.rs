//! Surround / double-vote / equivocation detector unit tests.

use consensus::{
    macro_fin::vote_book::{VoteBook, VoteRecord},
    slashing::{double_vote, surround},
};
use crypto::hash::dst;
use types::{
    crypto_types::{Hash32, VrfPubkey},
    macros::MacroProposal,
    primitives::{Epoch, Height, ValidatorId},
    slashing::SlashEvidence,
    validator::{ValidatorEntry, ValidatorIdentity, ValidatorSet},
};

fn test_set() -> (ValidatorSet, crypto::bls::SecretKey, ValidatorId) {
    let sk = crypto::bls::SecretKey::from_ikm(&[0x55; 32]).unwrap();
    let id = ValidatorId([0xAB; 32]);
    let set = ValidatorSet {
        epoch: Epoch(0),
        entries: vec![ValidatorEntry {
            id,
            bls_pubkey: sk.public().to_bytes(),
            vrf_pubkey: VrfPubkey::zero(),
            stake: types::primitives::StakeWeight(1),
            identity: ValidatorIdentity {
                asn: None,
                cloud: None,
                region: None,
            },
        }],
        total_stake: types::primitives::StakeWeight(1),
    };
    (set, sk, id)
}

fn signed_vote(
    sk: &crypto::bls::SecretKey,
    source: Epoch,
    target: Epoch,
    checkpoint: Hash32,
) -> VoteRecord {
    let record = VoteRecord {
        source,
        target,
        checkpoint,
        sig: types::crypto_types::BlsSig([0; 96]),
    };
    let msg = consensus::macro_fin::messages::vote_message(&record);
    VoteRecord {
        sig: crypto::bls::sign::sign(sk, dst::MACRO_VOTE, &msg),
        ..record
    }
}

#[test]
fn surround_vote_detected() {
    let (_set, sk, id) = test_set();
    let mut book = VoteBook::new();
    book.record(id, signed_vote(&sk, Epoch(1), Epoch(3), Hash32([1; 32])));
    book.record(id, signed_vote(&sk, Epoch(0), Epoch(5), Hash32([2; 32])));
    let ev = surround::scan_for_surround(&book, &id)
        .unwrap()
        .expect("surround");
    assert_eq!(ev.validator, id);
}

#[test]
fn double_vote_detected() {
    let (_set, sk, id) = test_set();
    let mut book = VoteBook::new();
    book.record(id, signed_vote(&sk, Epoch(4), Epoch(5), Hash32([1; 32])));
    book.record(id, signed_vote(&sk, Epoch(4), Epoch(5), Hash32([2; 32])));
    let ev = double_vote::scan_for_double_vote(&book, &id)
        .unwrap()
        .expect("double vote");
    assert_eq!(ev.target, Epoch(5));
}

#[test]
fn equivocation_evidence_verifies() {
    let (set, sk, id) = test_set();
    let cp_a = types::macros::MacroCheckpoint {
        height: Height(1),
        epoch: Epoch(0),
        parent: Hash32::zero(),
        micro_root: Hash32([0x11; 32]),
        hash: Hash32([0xAA; 32]),
    };
    let cp_b = types::macros::MacroCheckpoint {
        micro_root: Hash32([0x22; 32]),
        hash: Hash32([0xBB; 32]),
        ..cp_a.clone()
    };
    let sig_a = crypto::bls::sign::sign(
        &sk,
        dst::MACRO_PROPOSER_SIG,
        &consensus::macro_fin::messages::proposer_message(&id, &cp_a),
    );
    let sig_b = crypto::bls::sign::sign(
        &sk,
        dst::MACRO_PROPOSER_SIG,
        &consensus::macro_fin::messages::proposer_message(&id, &cp_b),
    );
    let ev = SlashEvidence::MacroEquivocation(consensus::slashing::equivocation::detect(
        id,
        MacroProposal {
            checkpoint: cp_a,
            proposer: id,
            vrf_proof: types::crypto_types::VrfProof([0; 80]),
            proposer_sig: sig_a,
        },
        MacroProposal {
            checkpoint: cp_b,
            proposer: id,
            vrf_proof: types::crypto_types::VrfProof([0; 80]),
            proposer_sig: sig_b,
        },
    ));
    consensus::slashing::verify_evidence(&ev, &set).unwrap();
}
