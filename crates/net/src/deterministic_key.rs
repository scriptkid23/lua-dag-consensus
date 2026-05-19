//! Deterministic (dev-only) libp2p keys from textual labels (spec §3.4).

use crypto::hash::{blake3_with_dst, dst};
use libp2p::identity::Keypair;
use types::crypto_types::Hash32;

use crate::error::{Error, Result};

/// Derive a libp2p Ed25519 keypair deterministically from a textual label.
///
/// Use **only** for the `devnet` profile. Never wire this into testnet/prod
/// — those profiles MUST mount real keypairs (spec §3.4 option 2).
///
/// Collision resistance comes from BLAKE3 + the fixed DST separator
/// (`dst::DEVNET_PEER_IDENTITY`).
pub fn devnet_keypair_from_label(label: &str) -> Result<Keypair> {
    let Hash32(mut bytes) = blake3_with_dst(dst::DEVNET_PEER_IDENTITY, label.as_bytes());
    // `ed25519_from_bytes` on a 32-byte seed cannot fail for any input the
    // BLAKE3 output can produce; the `Result` exists for API symmetry. If a
    // future libp2p version tightens validation, surface it as a Transport
    // error so the caller can refuse to start.
    Keypair::ed25519_from_bytes(&mut bytes)
        .map_err(|e| Error::Transport(format!("ed25519 key rejected for label `{label}`: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_label_yields_same_peer_id() {
        let a = devnet_keypair_from_label("node0").unwrap();
        let b = devnet_keypair_from_label("node0").unwrap();
        assert_eq!(a.public().to_peer_id(), b.public().to_peer_id());
    }

    #[test]
    fn different_labels_yield_different_peer_ids() {
        let a = devnet_keypair_from_label("node0").unwrap();
        let b = devnet_keypair_from_label("node1").unwrap();
        assert_ne!(a.public().to_peer_id(), b.public().to_peer_id());
    }
}
