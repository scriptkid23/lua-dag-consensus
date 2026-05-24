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
    /// Macro checkpoint hashing (L3 §5.6).
    pub const MACRO_CHECKPOINT: &[u8] = b"lua-dag/v1/macro-checkpoint";
    /// Macro window `micro_root` derivation (L3 §5.7).
    pub const MACRO_MICRO_ROOT: &[u8] = b"lua-dag/v1/macro-micro-root";
    /// Validator BLS partial signature (L3 03c-1 fixture).
    pub const VALIDATOR_BLS_PARTIAL: &[u8] = b"lua-dag/v1/validator-bls-partial";
    /// Macro proposer signature (L3 03c-1 fixture).
    pub const MACRO_PROPOSER_SIG: &[u8] = b"lua-dag/v1/macro-proposer-sig";
    /// Production vertex content hash (L1 07a).
    pub const VERTEX_HASH: &[u8] = b"lua-dag/v1/vertex-hash";
    /// BLS quorum certificate domain for certified vertices (L1 07a).
    pub const VERTEX_CERT: &[u8] = b"lua-dag/v1/vertex-cert";
    /// Content-addressed blob identifier (L1 07b).
    pub const BLOB_ID: &[u8] = b"lua-dag/v1/blob-id";
    /// Payload commitment for BlobRef (L1 07b; RS/KZG deferred to 07c).
    pub const BLOB_COMMIT: &[u8] = b"lua-dag/v1/blob-commit";
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

/// Deterministic 96-byte pseudo-BLS signature for 03c-1 fixture aggregation.
///
/// Produced by chaining three `blake3_with_dst` calls with a counter byte so
/// the output is wire-compatible with `BlsSig` (96 bytes). Real BLS sign /
/// verify arrives in plan 03d.
#[must_use]
pub fn fixture_bls_sig(dst: &[u8], parts: &[&[u8]]) -> [u8; 96] {
    let mut buf = Vec::with_capacity(parts.iter().map(|p| p.len()).sum::<usize>() + 1);
    for p in parts {
        buf.extend_from_slice(p);
    }
    let mut sig = [0u8; 96];
    for (i, chunk) in sig.chunks_mut(32).enumerate() {
        buf.push(u8::try_from(i).expect("3 chunks"));
        let h = blake3_with_dst(dst, &buf);
        chunk.copy_from_slice(&h.0);
        buf.pop();
    }
    sig
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
    fn new_l3_dsts_are_unique_and_distinct_from_existing() {
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
            dst::SIM_VERTEX_HASH,
            dst::MACRO_CHECKPOINT,
            dst::MACRO_MICRO_ROOT,
            dst::VALIDATOR_BLS_PARTIAL,
            dst::MACRO_PROPOSER_SIG,
            dst::VERTEX_HASH,
            dst::VERTEX_CERT,
            dst::BLOB_ID,
            dst::BLOB_COMMIT,
        ];
        let set: HashSet<&[u8]> = ids.iter().copied().collect();
        assert_eq!(set.len(), ids.len(), "DST registry has a duplicate");
    }

    #[test]
    fn fixture_bls_sig_is_deterministic_and_96_bytes() {
        let a = fixture_bls_sig(dst::VALIDATOR_BLS_PARTIAL, &[b"x", b"y"]);
        let b = fixture_bls_sig(dst::VALIDATOR_BLS_PARTIAL, &[b"x", b"y"]);
        assert_eq!(a, b);
        assert_eq!(a.len(), 96);
        let c = fixture_bls_sig(dst::MACRO_PROPOSER_SIG, &[b"x", b"y"]);
        assert_ne!(a, c, "different DST must change the fixture");
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
