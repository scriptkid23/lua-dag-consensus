//! Crate-level error type for `net`.

use thiserror::Error;

/// All failures from `crates/net`.
#[derive(Debug, Error)]
pub enum Error {
    /// libp2p transport setup failed.
    #[error("transport error: {0}")]
    Transport(String),

    /// Codec failure decoding a wire message.
    #[error("codec error: {0}")]
    Codec(String),

    /// Bridge channel was closed unexpectedly.
    #[error("bridge channel closed")]
    BridgeClosed,

    /// Consensus refused an event (e.g. `lock_macro` violation).
    #[error("consensus error: {0}")]
    Consensus(#[from] consensus::Error),
}

/// Result alias.
pub type Result<T> = core::result::Result<T, Error>;
