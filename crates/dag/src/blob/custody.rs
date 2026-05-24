use std::collections::{HashMap, HashSet};

use types::primitives::BlobId;

use crate::blob::store::BlobStore;

#[derive(Debug, Clone)]
struct BlobMeta {
    total_chunks: u32,
    #[allow(dead_code)]
    size_bytes: u64,
}

/// In-memory custody ledger; completeness is verified against a [`BlobStore`].
#[derive(Debug, Default)]
pub struct CustodyLedger {
    meta: HashMap<BlobId, BlobMeta>,
    available: HashSet<BlobId>,
}

impl CustodyLedger {
    /// Register expected chunk count for a blob.
    pub fn register_meta(&mut self, blob_id: BlobId, total_chunks: u32, size_bytes: u64) {
        self.meta
            .entry(blob_id)
            .and_modify(|m| {
                m.total_chunks = total_chunks;
                m.size_bytes = size_bytes;
            })
            .or_insert(BlobMeta {
                total_chunks,
                size_bytes,
            });
    }

    /// Record that chunk `index` was ingested; returns `true` when the blob
    /// newly transitions to locally available (all indices present in store).
    pub fn note_chunk(&mut self, blob_id: &BlobId, index: u32, store: &dyn BlobStore) -> bool {
        if self.available.contains(blob_id) {
            return false;
        }
        let Some(meta) = self.meta.get(blob_id) else {
            return false;
        };
        let _ = index;
        if !blob_complete(blob_id, meta.total_chunks, store) {
            return false;
        }
        self.available.insert(*blob_id);
        true
    }

    /// Whether all chunks for `blob_id` are stored locally.
    #[must_use]
    pub fn is_available(&self, blob_id: &BlobId) -> bool {
        self.available.contains(blob_id)
    }
}

fn blob_complete(blob_id: &BlobId, total_chunks: u32, store: &dyn BlobStore) -> bool {
    (0..total_chunks).all(|i| store.has_chunk(blob_id, i).unwrap_or(false))
}
