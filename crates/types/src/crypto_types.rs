//! Opaque byte wrappers for cryptographic material.
//!
//! These types **do not verify themselves** — they are storage and codec
//! shapes. Semantic operations live in `crates/crypto`.
//!
//! Per the crate tech-stack note, `serde` is reserved for config and
//! JSON-RPC interop. Wire codecs use Borsh for most crypto blobs; fixed-size
//! types that appear in config/bootstrap TOML (e.g. [`BlsPubkey`]) also
//! derive `serde`. `Hash32` is included for the same reason.

use core::fmt;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// BLS12-381 G1 compressed public key (48 bytes).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, BorshSerialize, BorshDeserialize)]
pub struct BlsPubkey(pub [u8; 48]);

impl Serialize for BlsPubkey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.serialize_str(&hex::encode(self.0))
        } else {
            serializer.serialize_bytes(&self.0)
        }
    }
}

impl<'de> Deserialize<'de> for BlsPubkey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if deserializer.is_human_readable() {
            deserializer.deserialize_str(BlsPubkeyVisitor)
        } else {
            deserializer.deserialize_bytes(BlsPubkeyVisitor)
        }
    }
}

struct BlsPubkeyVisitor;

impl<'de> Visitor<'de> for BlsPubkeyVisitor {
    type Value = BlsPubkey;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("48-byte BLS public key (hex string or raw bytes)")
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        let bytes = hex::decode(v.trim()).map_err(E::custom)?;
        if bytes.len() != 48 {
            return Err(E::invalid_length(bytes.len(), &"48 bytes after hex decode"));
        }
        let mut a = [0u8; 48];
        a.copy_from_slice(&bytes);
        Ok(BlsPubkey(a))
    }

    fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
        if v.len() != 48 {
            return Err(E::invalid_length(v.len(), &"exactly 48 bytes"));
        }
        let mut a = [0u8; 48];
        a.copy_from_slice(v);
        Ok(BlsPubkey(a))
    }

    fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut arr = [0u8; 48];
        for (i, slot) in arr.iter_mut().enumerate() {
            *slot = seq
                .next_element()?
                .ok_or_else(|| de::Error::invalid_length(i, &"sequence of 48 bytes"))?;
        }
        Ok(BlsPubkey(arr))
    }
}

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

/// Edwards25519 compressed public key for ECVRF (32 bytes).
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    PartialEq,
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
)]
pub struct VrfPubkey(pub [u8; 32]);

impl VrfPubkey {
    /// All-zero pubkey (tests / unset).
    #[must_use]
    pub const fn zero() -> Self {
        Self([0; 32])
    }

    /// True when no VRF key is configured.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; 32]
    }
}

impl VrfProof {
    /// All-zero proof; placeholder until real ECVRF sortition lands in 03c-2.
    #[must_use]
    pub const fn zero() -> Self {
        Self([0; 80])
    }
}

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
