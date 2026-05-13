//! Crate-level error type.

use thiserror::Error;

/// Errors that can be produced by `types` operations (mostly codec).
#[derive(Debug, Error)]
pub enum Error {
    /// Borsh serialization or deserialization failed.
    #[error("borsh codec failure: {0}")]
    Codec(String),

    /// A byte slice did not match the expected fixed length.
    #[error("invalid length: expected {expected} bytes, got {actual}")]
    InvalidLength {
        /// Expected byte length.
        expected: usize,
        /// Actual byte length encountered.
        actual: usize,
    },

    /// A value was outside its allowed domain (e.g. zero stake weight).
    #[error("value out of range: {0}")]
    OutOfRange(&'static str),
}

/// Convenience alias used throughout the crate.
pub type Result<T> = core::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_length_message_is_specific() {
        let e = Error::InvalidLength {
            expected: 32,
            actual: 16,
        };
        assert_eq!(e.to_string(), "invalid length: expected 32 bytes, got 16");
    }

    #[test]
    fn out_of_range_carries_static_message() {
        let e = Error::OutOfRange("stake_weight must be non-zero");
        assert!(e.to_string().contains("stake_weight"));
    }
}
