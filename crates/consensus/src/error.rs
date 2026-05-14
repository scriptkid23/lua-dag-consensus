//! Crate-level error type for consensus.

use thiserror::Error;

/// Consensus errors.
#[derive(Debug, Error)]
pub enum Error {
    /// Config TOML failed to parse or was missing fields.
    #[error("invalid config: {0}")]
    InvalidConfig(String),

    /// Storage / persistence port reported a failure.
    #[error("persistence error: {0}")]
    Persistence(String),

    /// Cryptographic primitive returned an error.
    #[error("crypto error: {0}")]
    Crypto(#[from] crypto::Error),

    /// Types codec / range error.
    #[error("types error: {0}")]
    Types(#[from] types::Error),

    /// `lock_macro` invariant was violated by an attempted action.
    #[error("lock_macro violation: {0}")]
    LockMacro(&'static str),
}

/// Result alias.
pub type Result<T> = core::result::Result<T, Error>;
