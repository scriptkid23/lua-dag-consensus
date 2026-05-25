use crypto::hash::{blake3_with_dst, dst};
use dag::{cert, devnet, signing};
use types::{
    crypto_types::Hash32,
    dag::Vertex,
    primitives::{Epoch, Round, StakeWeight, ValidatorId},
    validator::{ValidatorEntry, ValidatorIdentity, ValidatorSet},
};

fn devnet_valset() -> ValidatorSet {
    let entries = ["node0", "node1", "node2", "node3"]
        .into_iter()
        .map(|label| {
            let ikm = devnet::devnet_bls_ikm(label);
            let sk = crypto::bls::SecretKey::from_ikm(&ikm).unwrap();
            ValidatorEntry {
                id: ValidatorId(blake3_with_dst(dst::DEVNET_PEER_IDENTITY, label.as_bytes()).0),
                bls_pubkey: sk.public().to_bytes(),
                vrf_pubkey: types::crypto_types::VrfPubkey([0u8; 32]),
                stake: StakeWeight(1),
                identity: ValidatorIdentity {
                    asn: None,
                    cloud: None,
                    region: None,
                },
            }
        })
        .collect();
    ValidatorSet {
        epoch: Epoch(0),
        total_stake: StakeWeight(4),
        entries,
    }
}

#[test]
fn build_and_verify_quorum_cert() {
    let valset = devnet_valset();
    let author = valset.entries[0].id;
    let mut vertex = Vertex {
        round: Round(0),
        author,
        parents: vec![],
        blobs: vec![],
        hash: Hash32([0u8; 32]),
    };
    signing::seal_hash(&mut vertex);
    let signer_indices: Vec<u32> = vec![0, 1, 2];
    let cv = cert::build_quorum_cert(&vertex, &valset, &signer_indices).unwrap();
    cert::verify_certified_vertex(&cv, &valset).unwrap();
}

#[test]
fn tampered_hash_fails_verify() {
    let valset = devnet_valset();
    let mut vertex = Vertex {
        round: Round(1),
        author: valset.entries[1].id,
        parents: vec![],
        blobs: vec![],
        hash: Hash32([0u8; 32]),
    };
    signing::seal_hash(&mut vertex);
    let cv = cert::build_quorum_cert(&vertex, &valset, &[0, 1, 2]).unwrap();
    let mut bad = cv.clone();
    bad.vertex.hash = Hash32([0xFFu8; 32]);
    let err = cert::verify_certified_vertex(&bad, &valset).unwrap_err();
    assert!(err.to_string().contains("hash"));
}
