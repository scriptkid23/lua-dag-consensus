//! Devnet-only key lookup for multi-signer quorum certs in phase 07a.

use crypto::hash::{blake3_with_dst, dst};
use types::primitives::ValidatorId;

/// Devnet label for a validator id (`node0`..`node3`).
pub fn devnet_label_for_validator_id(id: &ValidatorId) -> Option<&'static str> {
    for label in ["node0", "node1", "node2", "node3"] {
        let h = blake3_with_dst(dst::DEVNET_PEER_IDENTITY, label.as_bytes());
        if h.0 == id.0 {
            return Some(label);
        }
    }
    None
}

/// BLS IKM for a devnet label (mirrors `apps/node/src/devnet_keys.rs`).
#[must_use]
pub fn devnet_bls_ikm(label: &str) -> [u8; 32] {
    blake3_with_dst(dst::VALIDATOR_BLS_PARTIAL, label.as_bytes()).0
}
