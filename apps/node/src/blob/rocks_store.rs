use std::sync::Arc;

use dag::blob::chunk::BlobChunk;
use dag::blob::store::{BlobStore, StoreError};
use storage::{db::Database, stores::blob_chunk_store};
use types::primitives::BlobId;

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
            chunk.index,
            chunk.total_chunks,
            chunk.size_bytes,
            &chunk.data,
        )
        .map_err(|e| StoreError::Other(e.to_string()))
    }

    fn get_chunk(&self, blob_id: &BlobId, index: u32) -> Result<Option<Vec<u8>>, StoreError> {
        blob_chunk_store::get(&self.db, blob_id, index).map_err(|e| StoreError::Other(e.to_string()))
    }

    fn has_chunk(&self, blob_id: &BlobId, index: u32) -> Result<bool, StoreError> {
        blob_chunk_store::has(&self.db, blob_id, index).map_err(|e| StoreError::Other(e.to_string()))
    }
}
