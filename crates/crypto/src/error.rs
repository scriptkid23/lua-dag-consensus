//! Crate-level error type for cryptographic operations.

use thiserror::Error;

/// All failures from `crates/crypto`.
#[derive(Debug, Error)]
pub enum Error {
    /// BLS signature failed to verify.
    #[error("BLS signature verification failed")]
    BlsVerifyFailed,

    /// BLS aggregation could not combine the supplied signatures.
    #[error("BLS aggregation failed: {0}")]
    BlsAggregateFailed(&'static str),

    /// Proof-of-Possession invalid.
    #[error("Proof-of-Possession invalid")]
    PopInvalid,

    /// VRF proof failed to verify.
    #[error("VRF proof verification failed")]
    VrfVerifyFailed,

    /// Bitmap length disagrees with validator count.
    #[error("bitmap length mismatch: bitmap covers {bitmap_bits} bits, expected {expected}")]
    BitmapLength {
        /// Number of bits the supplied bitmap exposes.
        bitmap_bits: usize,
        /// Validator count the caller is operating over.
        expected: usize,
    },

    /// Stake-weighted sortition rejection (caller-loop should retry with next `y_i`).
    #[error("sortition rejection")]
    SortitionRejected,

    /// Encoding/decoding error from `types`.
    #[error("types codec error: {0}")]
    Types(#[from] types::Error),
}

/// Convenience result alias.
pub type Result<T> = core::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmap_length_error_shows_both_sides() {
        let e = Error::BitmapLength {
            bitmap_bits: 256,
            expected: 300,
        };
        let s = e.to_string();
        assert!(s.contains("256"));
        assert!(s.contains("300"));
    }
}
