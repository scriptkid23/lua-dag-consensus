//! Numeric and identity primitives used across the protocol.

use core::fmt;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

macro_rules! u64_newtype {
    ($(#[$m:meta])* $name:ident, $alias:literal) => {
        $(#[$m])*
        #[derive(
            Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash,
            BorshSerialize, BorshDeserialize, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub u64);

        impl $name {
            #[doc = concat!("Create a new ", $alias, ".")]
            #[must_use]
            pub const fn new(v: u64) -> Self { Self(v) }
            /// Inner `u64`.
            #[must_use]
            pub const fn get(self) -> u64 { self.0 }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", $alias, self.0)
            }
        }

        impl From<u64> for $name { fn from(v: u64) -> Self { Self(v) } }
        impl From<$name> for u64 { fn from(v: $name) -> Self { v.0 } }
    };
}

u64_newtype!(/// Micro-slot round number (4 per Bullshark wave).
    Round, "Round");
u64_newtype!(/// Macro window height; monotonically increasing.
    Height, "Height");
u64_newtype!(/// Validator-set epoch.
    Epoch, "Epoch");
u64_newtype!(/// Stake weight (whole-number units; conversion to fractions done at use sites).
    StakeWeight, "StakeWeight");

/// 256-bit validator identifier (typically `H(BlsPubkey)`).
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
pub struct ValidatorId(pub [u8; 32]);

impl ValidatorId {
    /// Build from a fixed 32-byte array.
    #[must_use]
    pub const fn from_bytes(b: [u8; 32]) -> Self {
        Self(b)
    }
    /// Borrow the underlying bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for ValidatorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "validator:{}", hex::encode(&self.0[..8]))
    }
}

/// 256-bit blob identifier (typically `H(blob bytes)`).
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
pub struct BlobId(pub [u8; 32]);

impl BlobId {
    /// Build from a fixed 32-byte array.
    #[must_use]
    pub const fn from_bytes(b: [u8; 32]) -> Self {
        Self(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use borsh::to_vec;

    #[test]
    fn round_display_uses_label() {
        assert_eq!(Round(7).to_string(), "Round(7)");
    }

    #[test]
    fn validator_id_display_truncates_to_8_bytes() {
        let v = ValidatorId([0xAB; 32]);
        let s = v.to_string();
        assert!(s.starts_with("validator:abababab"));
    }

    #[test]
    fn primitives_round_trip_borsh() {
        let r = Round(42);
        let bytes = to_vec(&r).unwrap();
        let r2: Round = borsh::from_slice(&bytes).unwrap();
        assert_eq!(r, r2);

        let v = ValidatorId([7; 32]);
        let bytes = to_vec(&v).unwrap();
        let v2: ValidatorId = borsh::from_slice(&bytes).unwrap();
        assert_eq!(v, v2);
    }
}
