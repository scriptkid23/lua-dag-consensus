use std::sync::Arc;

use dag::blob::chunk::BlobChunk;
use dag::blob::store::{BlobStore, StoreError};
use storage::{db::Database, stores::blob_chunk_store};
use types::{dag::ChunkRef, primitives::BlobId};

/// RocksDB-backed chunk store for [`BlobStore`].
pub struct RocksBlobStore {
    db: Arc<Database>,
}

impl RocksBlobStore {
    /// Wrap an opened [`Database`].
    #[must_use]
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

impl BlobStore for RocksBlobStore {
    fn put_chunk(&self, chunk: &BlobChunk) -> Result<(), StoreError> {
        blob_chunk_store::put(
            &self.db,
            &chunk.blob_id,
            chunk.index(),
            chunk.unit_count(),
            chunk.size_bytes,
            chunk_payload_bytes(chunk),
        )
        .map_err(|e| StoreError::Other(e.to_string()))
    }

    fn get_chunk(&self, blob_id: &BlobId, index: u32) -> Result<Option<Vec<u8>>, StoreError> {
        blob_chunk_store::get(&self.db, blob_id, index).map_err(|e| StoreError::Other(e.to_string()))
    }

    fn has_chunk(&self, blob_id: &BlobId, index: u32) -> Result<bool, StoreError> {
        blob_chunk_store::has(&self.db, blob_id, index).map_err(|e| StoreError::Other(e.to_string()))
    }

    fn list_chunk_refs(&self, blob_id: &BlobId) -> Result<Vec<ChunkRef>, StoreError> {
        let mut out = Vec::new();
        for index in 0..64u32 {
            if self.has_chunk(blob_id, index)? {
                out.push(ChunkRef {
                    blob_id: *blob_id,
                    index,
                });
            }
        }
        Ok(out)
    }
}

fn chunk_payload_bytes(chunk: &BlobChunk) -> &[u8] {
    match &chunk.payload {
        dag::blob::chunk::ChunkPayload::Sequential { data, .. }
        | dag::blob::chunk::ChunkPayload::Erasure { data, .. } => data,
    }
}
