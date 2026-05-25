//! Tampered certified vertices must not verify when real certs are enabled.

use dag::{cert, signing};
use node::{
    devnet_keys::devnet_valset_four,
    l1::vertex_builder::{build_certified_vertex, sim_vertex_hash},
};
use types::{
    crypto_types::{BlsAggSig, BlsSig, Hash32},
    dag::Vertex,
    primitives::Round,
};

#[test]
fn verify_rejects_fixture_cert_when_real_certs_enabled() {
    let valset = devnet_valset_four();
    let author = valset.entries[0].id;
    let cv = build_certified_vertex(0, author, None, false, &valset);
    assert!(cert::verify_certified_vertex(&cv, &valset).is_err());
}

#[test]
fn fixture_hash_with_sealed_body_still_fails_bls_verify() {
    let valset = devnet_valset_four();
    let author = valset.entries[0].id;
    let mut vertex = Vertex {
        round: Round(1),
        author,
        parents: vec![],
        blobs: vec![],
        hash: sim_vertex_hash(1, &author),
    };
    signing::seal_hash(&mut vertex);
    let cv = types::dag::CertifiedVertex {
        vertex,
        certificate: BlsAggSig {
            sig: BlsSig([0xAB; 96]),
            bitmap: vec![0xFF],
        },
    };
    assert!(cert::verify_certified_vertex(&cv, &valset).is_err());
}

#[test]
fn real_cert_from_builder_verifies() {
    let valset = devnet_valset_four();
    let author = valset.entries[0].id;
    let cv = build_certified_vertex(2, author, None, true, &valset);
    cert::verify_certified_vertex(&cv, &valset).expect("real cert must verify");
}
