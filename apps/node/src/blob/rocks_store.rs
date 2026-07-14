use std::sync::Arc;

use dag::blob::chunk::BlobChunk;
use dag::blob::store::{BlobStore, StoreError};
use storage::{
    db::Database,
    stores::{blob_chunk_store, blob_publish_store},
    wal::{self, new_batch},
};
use types::{dag::BlobRef, primitives::BlobId};

pub use storage::stores::blob_publish_store::{PublishRecord, PublishState};

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

    /// Borrow the underlying database (boot orphan scan).
    #[must_use]
    pub fn db(&self) -> &Database {
        &self.db
    }

    /// Atomically persist all chunks plus a `Ready` publish record.
    pub fn publish_blob_atomic(
        &self,
        chunks: &[BlobChunk],
        record: PublishRecord,
    ) -> Result<(), StoreError> {
        let mut batch = new_batch();
        for chunk in chunks {
            blob_chunk_store::put_batch(
                &mut batch,
                &self.db,
                &chunk.blob_id,
                chunk.index(),
                chunk.unit_count(),
                chunk.size_bytes,
                chunk_payload_bytes(chunk),
            )
            .map_err(|e| StoreError::Other(e.to_string()))?;
        }
        blob_publish_store::put_ready_batch(
            &mut batch,
            &self.db,
            &record.blob_ref.blob_id,
            &record,
        )
        .map_err(|e| StoreError::Other(e.to_string()))?;
        wal::apply(&self.db, batch).map_err(|e| StoreError::Other(e.to_string()))?;
        Ok(())
    }

    /// Transition publish state to `Attached` (idempotent).
    pub fn mark_attached(&self, blob_id: &BlobId) -> Result<(), StoreError> {
        blob_publish_store::put_attached(&self.db, blob_id)
            .map_err(|e| StoreError::Other(e.to_string()))
    }

    /// Whether `blob_id` is in `Attached` publish state.
    pub fn is_attached(&self, blob_id: &BlobId) -> Result<bool, StoreError> {
        blob_publish_store::is_attached(&self.db, blob_id)
            .map_err(|e| StoreError::Other(e.to_string()))
    }

    /// All `BlobRef` rows in `Ready` state (boot recovery).
    pub fn scan_ready_blobs(&self) -> Result<Vec<BlobRef>, StoreError> {
        blob_publish_store::scan_ready(&self.db).map_err(|e| StoreError::Other(e.to_string()))
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

    fn list_chunk_refs(&self, blob_id: &BlobId) -> Result<Vec<types::dag::ChunkRef>, StoreError> {
        let mut out = Vec::new();
        for index in 0..64u32 {
            if self.has_chunk(blob_id, index)? {
                out.push(types::dag::ChunkRef {
                    blob_id: *blob_id,
                    index,
                });
            }
        }
        Ok(out)
    }
}

fn chunk_payload_bytes(chunk: &BlobChunk) -> &[u8] {
    let dag::blob::chunk::ChunkPayload::Erasure { data, .. } = &chunk.payload;
    data
}

#[cfg(test)]
mod tests {
    use super::*;
    use dag::blob::chunk::{BlobChunk, ChunkPayload};
    use storage::config::StorageConfig;
    use tempfile::tempdir;
    use types::crypto_types::Hash32;

    fn test_store() -> (tempfile::TempDir, RocksBlobStore) {
        let dir = tempdir().unwrap();
        let db = Arc::new(
            Database::open(&StorageConfig {
                path: dir.path().to_path_buf(),
                create_if_missing: true,
                max_total_wal_size_mb: 16,
            })
            .unwrap(),
        );
        (dir, RocksBlobStore::new(db))
    }

    fn sample_chunk(blob_id: BlobId, index: u32) -> BlobChunk {
        BlobChunk {
            blob_id,
            size_bytes: 1024,
            payload: ChunkPayload::Erasure {
                index,
                n_shards: 4,
                data: format!("chunk-{index}").into_bytes(),
            },
        }
    }

    #[test]
    fn atomic_publish_all_or_nothing() {
        let (_dir, store) = test_store();
        let blob_id = BlobId([0x11; 32]);
        let blob_ref = BlobRef {
            blob_id,
            commitment: Hash32([0x22; 32]),
            size_bytes: 1024,
        };
        let record = PublishRecord {
            state: PublishState::Ready as u8,
            blob_ref,
        };
        let chunks = vec![sample_chunk(blob_id, 0), sample_chunk(blob_id, 1)];
        store.publish_blob_atomic(&chunks, record).unwrap();
        assert!(store.has_chunk(&blob_id, 0).unwrap());
        assert!(store.has_chunk(&blob_id, 1).unwrap());
        assert!(!store.is_attached(&blob_id).unwrap());
        assert_eq!(store.scan_ready_blobs().unwrap().len(), 1);
    }
}
