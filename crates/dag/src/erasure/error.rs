use thiserror::Error;

/// Erasure codec failures.
#[derive(Debug, Error)]
pub enum ErasureError {
    /// Invalid configuration.
    #[error("invalid erasure config: {0}")]
    Config(&'static str),
    /// Reed–Solomon engine error.
    #[error("reed-solomon error: {0}")]
    Codec(String),
}

pub type Result<T> = core::result::Result<T, ErasureError>;
