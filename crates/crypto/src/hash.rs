//! Hashing primitives + global domain-separation tag (DST) registry.
//!
//! Every protocol message that gets hashed or signed picks one DST from
//! [`dst`]. New DSTs are appended; existing values must never change.

use sha2::{Digest, Sha256};
use types::crypto_types::Hash32;

/// Centralised DST registry.
///
/// `DST_*` constants are appended only; never edit an existing value or
/// the wire format changes.
pub mod dst {
    /// Generic content addressing inside `crates/types`.
    pub const CONTENT_HASH: &[u8] = b"lua-dag/v1/content";
    /// Bullshark `MicroQc` message.
    pub const MICRO_QC: &[u8] = b"lua-dag/v1/micro-qc";
    /// Macro proposal signing root.
    pub const MACRO_PROPOSAL: &[u8] = b"lua-dag/v1/macro-proposal";
    /// Macro vote signing root.
    pub const MACRO_VOTE: &[u8] = b"lua-dag/v1/macro-vote";
    /// Beacon chaining input.
    pub const BEACON: &[u8] = b"lua-dag/v1/beacon";
    /// Subnet membership derivation.
    pub const SUBNET_ASSIGN: &[u8] = b"lua-dag/v1/subnet-assign";
    /// Proof-of-Possession.
    pub const POP: &[u8] = b"lua-dag/v1/pop";
    /// Deterministic peer key derivation for **`devnet` only** —
    /// blake3 input is hashed with this prefix before supplying bytes
    /// to libp2p Ed25519 key material.
    pub const DEVNET_PEER_IDENTITY: &[u8] = b"lua-dag/v1/devnet-peer-identity";
    /// Sim-only vertex hashing (factory in `apps/sim`). Distinct from
    /// [`CONTENT_HASH`] to keep production and sim namespaces isolated.
    pub const SIM_VERTEX_HASH: &[u8] = b"lua-dag/v1/sim-vertex-hash";
}

/// Blake3-256 over `data` with a DST prefix.
#[must_use]
pub fn blake3_with_dst(dst: &[u8], data: &[u8]) -> Hash32 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(dst);
    hasher.update(&[0x00]); // separator byte
    hasher.update(data);
    Hash32(*hasher.finalize().as_bytes())
}

/// SHA-256 over `data` with a DST prefix. Only for backwards-compat or
/// hash-to-curve flows that mandate SHA-256.
#[must_use]
pub fn sha256_with_dst(dst: &[u8], data: &[u8]) -> Hash32 {
    let mut hasher = Sha256::new();
    hasher.update(dst);
    hasher.update([0x00]);
    hasher.update(data);
    let out = hasher.finalize();
    let mut h = [0u8; 32];
    h.copy_from_slice(&out);
    Hash32(h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn different_dsts_yield_different_hashes() {
        let a = blake3_with_dst(dst::MICRO_QC, b"hello");
        let b = blake3_with_dst(dst::MACRO_VOTE, b"hello");
        assert_ne!(a, b, "DST must change the hash output");
    }

    #[test]
    fn blake3_is_deterministic() {
        let a = blake3_with_dst(dst::BEACON, b"x");
        let b = blake3_with_dst(dst::BEACON, b"x");
        assert_eq!(a, b);
    }

    #[test]
    fn sha256_is_deterministic_and_differs_from_blake3() {
        let a = sha256_with_dst(dst::POP, b"x");
        let b = sha256_with_dst(dst::POP, b"x");
        let c = blake3_with_dst(dst::POP, b"x");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn devnet_peer_identity_dst_is_unique() {
        use std::collections::HashSet;
        let ids: &[&[u8]] = &[
            dst::CONTENT_HASH,
            dst::MICRO_QC,
            dst::MACRO_PROPOSAL,
            dst::MACRO_VOTE,
            dst::BEACON,
            dst::SUBNET_ASSIGN,
            dst::POP,
            dst::DEVNET_PEER_IDENTITY,
        ];
        let set: HashSet<&[u8]> = ids.iter().copied().collect();
        assert_eq!(set.len(), ids.len(), "DST registry has a duplicate");
    }
}
