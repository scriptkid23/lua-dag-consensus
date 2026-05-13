//! Opaque byte wrappers for cryptographic material.
//!
//! These types **do not verify themselves** — they are storage and codec
//! shapes. Semantic operations live in `crates/crypto`.
//!
//! Per the crate tech-stack note, `serde` is reserved for config and
//! JSON-RPC interop. Crypto material travels on the wire via Borsh only,
//! so `Serialize`/`Deserialize` are deliberately not derived for the
//! variable- or oversized-array types below. `Hash32` keeps serde because
//! its 32-byte array is within serde's stock derive support and the type
//! shows up in config/RPC surfaces.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// BLS12-381 G1 compressed public key (48 bytes).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, BorshSerialize, BorshDeserialize)]
pub struct BlsPubkey(pub [u8; 48]);

/// BLS12-381 G2 compressed signature (96 bytes).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, BorshSerialize, BorshDeserialize)]
pub struct BlsSig(pub [u8; 96]);

/// Aggregated BLS signature plus bitmap of signers.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct BlsAggSig {
    /// Compressed G2 aggregate.
    pub sig: BlsSig,
    /// Bit `i` set iff validator index `i` is included.
    pub bitmap: Vec<u8>,
}

/// Proof-of-Possession for a BLS public key.
#[derive(Clone, Copy, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct Pop(pub BlsSig);

/// ECVRF (Edwards25519, RFC 9381) proof. 80 bytes per RFC.
#[derive(Clone, Copy, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct VrfProof(pub [u8; 80]);

/// 256-bit hash (Blake3 unless otherwise documented at the call site).
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
)]
pub struct Hash32(pub [u8; 32]);

impl Hash32 {
    /// Build from a fixed 32-byte array.
    #[must_use]
    pub const fn new(b: [u8; 32]) -> Self {
        Self(b)
    }
    /// Borrow the underlying bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
    /// All-zero hash; useful as a sentinel.
    #[must_use]
    pub const fn zero() -> Self {
        Self([0; 32])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use borsh::to_vec;

    #[test]
    fn bls_aggsig_round_trips_with_variable_bitmap() {
        let s = BlsAggSig {
            sig: BlsSig([1; 96]),
            bitmap: vec![0xFF, 0x0F],
        };
        let bytes = to_vec(&s).unwrap();
        let s2: BlsAggSig = borsh::from_slice(&bytes).unwrap();
        assert_eq!(s, s2);
    }

    #[test]
    fn hash32_zero_is_all_zero() {
        assert_eq!(Hash32::zero().as_bytes(), &[0; 32]);
    }

    #[test]
    fn vrf_proof_fixed_80_bytes() {
        let p = VrfProof([0; 80]);
        let bytes = to_vec(&p).unwrap();
        assert_eq!(bytes.len(), 80);
    }
}
