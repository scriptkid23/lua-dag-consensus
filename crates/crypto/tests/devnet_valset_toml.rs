//! One-shot generator for `config/valsets/devnet-4.toml` (plan 06b-l3).
//!
//! Key derivation matches `node::devnet_keys`:
//!   * `ValidatorId` = BLAKE3(`DEVNET_PEER_IDENTITY`, label)
//!   * BLS IKM     = BLAKE3(`VALIDATOR_BLS_PARTIAL`, label)
//!   * VRF seed    = BLAKE3(`MACRO_PROPOSER_SIG`, label)

use crypto::hash::{blake3_with_dst, dst};
use types::{
    crypto_types::VrfPubkey,
    primitives::{Epoch, StakeWeight, ValidatorId},
    validator::{ValidatorEntry, ValidatorIdentity, ValidatorSet},
};

fn entry(label: &str) -> ValidatorEntry {
    let ikm = blake3_with_dst(dst::VALIDATOR_BLS_PARTIAL, label.as_bytes()).0;
    let bls = crypto::bls::SecretKey::from_ikm(&ikm).unwrap();
    let vrf = crypto::vrf::VrfKey::from_seed(
        &blake3_with_dst(dst::MACRO_PROPOSER_SIG, label.as_bytes()).0,
    );
    ValidatorEntry {
        id: ValidatorId(blake3_with_dst(dst::DEVNET_PEER_IDENTITY, label.as_bytes()).0),
        bls_pubkey: bls.public().to_bytes(),
        vrf_pubkey: VrfPubkey(vrf.pubkey()),
        stake: StakeWeight(1),
        identity: ValidatorIdentity {
            asn: None,
            cloud: None,
            region: None,
        },
    }
}

#[test]
fn devnet_valset_four_matches_node_devnet_keys() {
    let entries: Vec<_> = ["node0", "node1", "node2", "node3"]
        .into_iter()
        .map(entry)
        .collect();
    let set = ValidatorSet {
        epoch: Epoch(0),
        entries,
        total_stake: StakeWeight(4),
    };
    let raw = toml::to_string(&set).unwrap();
    let back: ValidatorSet = toml::from_str(&raw).unwrap();
    assert_eq!(set, back);
}

#[test]
#[ignore = "run once to refresh config/valsets/devnet-4.toml"]
fn write_devnet_valset_toml_fixture() {
    let entries: Vec<_> = ["node0", "node1", "node2", "node3"]
        .into_iter()
        .map(entry)
        .collect();
    let set = ValidatorSet {
        epoch: Epoch(0),
        entries,
        total_stake: StakeWeight(4),
    };
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../config/valsets/devnet-4.toml");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, toml::to_string(&set).unwrap()).unwrap();
}
