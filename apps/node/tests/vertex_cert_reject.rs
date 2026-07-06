//! Tampered certified vertices must never verify.

use dag::{cert, signing};
use node::devnet_keys::devnet_valset_four;
use types::{
    crypto_types::{BlsAggSig, BlsSig, Hash32},
    dag::{CertifiedVertex, Vertex},
    primitives::Round,
};

fn fixture_certificate() -> BlsAggSig {
    BlsAggSig {
        sig: BlsSig([0xAB; 96]),
        bitmap: vec![0xFF],
    }
}

#[test]
fn unsealed_hash_fails_verify() {
    let valset = devnet_valset_four();
    let author = valset.entries[0].id;
    let vertex = Vertex {
        round: Round(0),
        author,
        parents: vec![],
        blobs: vec![],
        hash: Hash32([0x11; 32]), // not the sealed content hash
    };
    let cv = CertifiedVertex {
        vertex,
        certificate: fixture_certificate(),
    };
    assert!(cert::verify_certified_vertex(&cv, &valset).is_err());
}

#[test]
fn sealed_body_with_fixture_signature_fails_bls_verify() {
    let valset = devnet_valset_four();
    let author = valset.entries[0].id;
    let mut vertex = Vertex {
        round: Round(1),
        author,
        parents: vec![],
        blobs: vec![],
        hash: Hash32([0u8; 32]),
    };
    signing::seal_hash(&mut vertex);
    let cv = CertifiedVertex {
        vertex,
        certificate: fixture_certificate(),
    };
    assert!(cert::verify_certified_vertex(&cv, &valset).is_err());
}

#[test]
fn real_quorum_cert_verifies() {
    let valset = devnet_valset_four();
    let author = valset.entries[0].id;
    let mut vertex = Vertex {
        round: Round(2),
        author,
        parents: vec![],
        blobs: vec![],
        hash: Hash32([0u8; 32]),
    };
    signing::seal_hash(&mut vertex);
    let cv = cert::build_quorum_cert(&vertex, &valset, &[0, 1, 2]).expect("quorum cert builds");
    cert::verify_certified_vertex(&cv, &valset).expect("real cert must verify");
}
