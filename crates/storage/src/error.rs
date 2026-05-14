//! Crate-level error type.

use thiserror::Error;

/// All failures from `crates/storage`.
#[derive(Debug, Error)]
pub enum Error {
    /// `RocksDB` returned an error.
    #[error("rocksdb error: {0}")]
    Rocks(#[from] rocksdb::Error),

    /// Codec failure encoding/decoding a value.
    #[error("codec error: {0}")]
    Codec(String),

    /// Column family was missing from the open handle.
    #[error("unknown column family: {0}")]
    UnknownColumn(&'static str),

    /// Logical error (e.g. invariant violation).
    #[error("logic error: {0}")]
    Logic(&'static str),

    /// Wrapping a `types` error.
    #[error("types error: {0}")]
    Types(#[from] types::Error),
}

/// Convenience alias.
pub type Result<T> = core::result::Result<T, Error>;
