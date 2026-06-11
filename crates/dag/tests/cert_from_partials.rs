//! `assemble_cert` builds a verifying CV from externally collected partials.

use crypto::{bls::keys::SecretKey, bls::sign::sign, hash::dst};
use dag::{cert, signing};
use types::{
    crypto_types::{Hash32, VrfPubkey},
    dag::Vertex,
    primitives::{Epoch, Round, StakeWeight, ValidatorId},
    validator::{ValidatorEntry, ValidatorIdentity, ValidatorSet},
};

fn sk(i: u8) -> SecretKey {
    SecretKey::from_ikm(&[i; 32]).unwrap()
}

fn vset(n: u8) -> ValidatorSet {
    let entries = (0..n)
        .map(|i| ValidatorEntry {
            id: ValidatorId([i; 32]),
            bls_pubkey: sk(i).public().to_bytes(),
            vrf_pubkey: VrfPubkey::zero(),
            stake: StakeWeight(1),
            identity: ValidatorIdentity {
                asn: None,
                cloud: None,
                region: None,
            },
        })
        .collect();
    ValidatorSet {
        epoch: Epoch(0),
        entries,
        total_stake: StakeWeight(u64::from(n)),
    }
}

fn sealed_vertex(author: ValidatorId) -> Vertex {
    let mut v = Vertex {
        round: Round(1),
        author,
        parents: vec![Hash32([0xAA; 32])],
        blobs: vec![],
        hash: Hash32::zero(),
    };
    signing::seal_hash(&mut v);
    v
}

#[test]
fn quorum_threshold_is_public_and_correct() {
    assert_eq!(cert::quorum_threshold(4), 3);
    assert_eq!(cert::quorum_threshold(1), 1);
    assert_eq!(cert::quorum_threshold(7), 5);
}

#[test]
fn assemble_cert_from_three_partials_verifies() {
    let set = vset(4);
    let vertex = sealed_vertex(set.entries[0].id);
    let msg = signing::signing_bytes(&vertex);
    let contributors: Vec<(u32, types::crypto_types::BlsSig)> = [0u8, 1, 2]
        .iter()
        .map(|&i| (u32::from(i), sign(&sk(i), dst::VERTEX_CERT, &msg)))
        .collect();
    let cv = cert::assemble_cert(&vertex, &set, &contributors).unwrap();
    cert::verify_certified_vertex(&cv, &set).expect("assembled cert must verify");
}

#[test]
fn assemble_cert_below_quorum_fails() {
    let set = vset(4);
    let vertex = sealed_vertex(set.entries[0].id);
    let msg = signing::signing_bytes(&vertex);
    let contributors = vec![(0u32, sign(&sk(0), dst::VERTEX_CERT, &msg))];
    assert!(matches!(
        cert::assemble_cert(&vertex, &set, &contributors),
        Err(cert::CertError::InsufficientSigners { got: 1, need: 3 })
    ));
}

#[test]
fn assemble_cert_rejects_out_of_range_index() {
    let set = vset(4);
    let vertex = sealed_vertex(set.entries[0].id);
    let msg = signing::signing_bytes(&vertex);
    let contributors: Vec<_> = [0u8, 1, 2]
        .iter()
        .map(|&i| (u32::from(i), sign(&sk(i), dst::VERTEX_CERT, &msg)))
        .chain(std::iter::once((
            9u32,
            sign(&sk(3), dst::VERTEX_CERT, &msg),
        )))
        .collect();
    assert!(matches!(
        cert::assemble_cert(&vertex, &set, &contributors),
        Err(cert::CertError::BadIndex(9))
    ));
}

#[test]
fn build_quorum_cert_with_still_works_after_refactor() {
    let set = vset(4);
    let vertex = sealed_vertex(set.entries[0].id);
    let cv = cert::build_quorum_cert_with(&vertex, &set, &[0, 1, 2], |i| {
        Ok(sk(u8::try_from(i).unwrap()))
    })
    .unwrap();
    cert::verify_certified_vertex(&cv, &set).expect("legacy path must still verify");
}
