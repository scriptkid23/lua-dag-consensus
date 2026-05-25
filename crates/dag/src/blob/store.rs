use thiserror::Error;
use types::{dag::ChunkRef, primitives::BlobId};

use crate::blob::chunk::BlobChunk;

/// Blob chunk persistence failures.
#[derive(Debug, Error)]
pub enum StoreError {
    /// Underlying store returned an error.
    #[error("store error: {0}")]
    Other(String),
}

/// Host-side blob chunk store (Rocks-backed in production).
pub trait BlobStore: Send + Sync {
    /// Persist one chunk.
    fn put_chunk(&self, chunk: &BlobChunk) -> Result<(), StoreError>;
    /// Fetch chunk bytes, if present.
    fn get_chunk(&self, blob_id: &BlobId, index: u32) -> Result<Option<Vec<u8>>, StoreError>;
    /// Whether chunk `index` exists for `blob_id`.
    fn has_chunk(&self, blob_id: &BlobId, index: u32) -> Result<bool, StoreError>;
    /// List stored chunk references for a blob.
    fn list_chunk_refs(&self, blob_id: &BlobId) -> Result<Vec<ChunkRef>, StoreError>;
}
