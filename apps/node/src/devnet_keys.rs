//! Deterministic devnet validator ids and key material (plan 06b-l3).
//!
//! Keys derive from the node identity label the same way `DevSigner` loads
//! when no override file/env is set:
//!   * BLS IKM  = BLAKE3(`VALIDATOR_BLS_PARTIAL`, label)
//!   * VRF seed = BLAKE3(`MACRO_PROPOSER_SIG`, label)
//!   * ValidatorId = BLAKE3(`DEVNET_PEER_IDENTITY`, label)

use crypto::hash::{blake3_with_dst, dst};
use types::{
    crypto_types::VrfPubkey,
    primitives::{Epoch, StakeWeight, ValidatorId},
    validator::{ValidatorEntry, ValidatorIdentity, ValidatorSet},
};

/// Derive the devnet `ValidatorId` for a node label.
#[must_use]
pub fn validator_id_from_label(label: &str) -> ValidatorId {
    let h = blake3_with_dst(dst::DEVNET_PEER_IDENTITY, label.as_bytes());
    ValidatorId(h.0)
}

/// BLS key-input keying material for a devnet label.
#[must_use]
pub fn devnet_bls_ikm(label: &str) -> [u8; 32] {
    blake3_with_dst(dst::VALIDATOR_BLS_PARTIAL, label.as_bytes()).0
}

/// ECVRF seed for a devnet label.
#[must_use]
pub fn devnet_vrf_seed(label: &str) -> [u8; 32] {
    blake3_with_dst(dst::MACRO_PROPOSER_SIG, label.as_bytes()).0
}

/// Build one devnet validator entry (stake defaults to 1).
#[must_use]
pub fn devnet_validator_entry(label: &str) -> ValidatorEntry {
    let ikm = devnet_bls_ikm(label);
    let bls = crypto::bls::SecretKey::from_ikm(&ikm).expect("devnet BLS IKM must be valid");
    let vrf = crypto::vrf::VrfKey::from_seed(&devnet_vrf_seed(label));
    ValidatorEntry {
        id: validator_id_from_label(label),
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

/// Four-validator devnet set (`node0`..`node3`).
#[must_use]
pub fn devnet_valset_four() -> ValidatorSet {
    let entries: Vec<_> = ["node0", "node1", "node2", "node3"]
        .into_iter()
        .map(devnet_validator_entry)
        .collect();
    let total_stake = StakeWeight(
        entries
            .iter()
            .map(|e| e.stake.get())
            .sum(),
    );
    ValidatorSet {
        epoch: Epoch(0),
        entries,
        total_stake,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devnet_valset_round_trips_toml() {
        let set = devnet_valset_four();
        let raw = toml::to_string(&set).unwrap();
        let back: ValidatorSet = toml::from_str(&raw).unwrap();
        assert_eq!(set, back);
    }
}
