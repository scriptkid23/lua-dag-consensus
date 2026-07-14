use std::collections::{HashMap, HashSet};

use types::primitives::BlobId;

use crate::blob::store::BlobStore;
use crate::erasure::{decode_shards, ErasureConfig};

#[derive(Debug, Clone)]
struct BlobMeta {
    size_bytes: u64,
    cfg: ErasureConfig,
    received: HashSet<u32>,
}

/// In-memory custody ledger; completeness is verified against a [`BlobStore`].
#[derive(Debug, Default)]
pub struct CustodyLedger {
    meta: HashMap<BlobId, BlobMeta>,
    available: HashSet<BlobId>,
}

impl CustodyLedger {
    /// Register erasure shard expectations (07c).
    pub fn register_erasure(
        &mut self,
        blob_id: BlobId,
        cfg: ErasureConfig,
        _n_shards: u32,
        size_bytes: u64,
    ) {
        self.meta
            .entry(blob_id)
            .and_modify(|m| {
                m.size_bytes = size_bytes;
                m.cfg = cfg;
            })
            .or_insert(BlobMeta {
                size_bytes,
                cfg,
                received: HashSet::new(),
            });
    }

    /// Record that shard `index` was ingested; returns `true` when the blob
    /// newly transitions to locally available.
    pub fn note_chunk(&mut self, blob_id: &BlobId, index: u32, store: &dyn BlobStore) -> bool {
        if self.available.contains(blob_id) {
            return false;
        }
        let Some(meta) = self.meta.get_mut(blob_id) else {
            return false;
        };
        meta.received.insert(index);
        if erasure_available(blob_id, meta, store) {
            self.available.insert(*blob_id);
            true
        } else {
            false
        }
    }

    /// Metadata-only shard presence (boot rehydrate; no payload reads).
    pub fn note_chunk_present(&mut self, blob_id: &BlobId, index: u32) -> bool {
        if self.available.contains(blob_id) {
            return false;
        }
        let Some(meta) = self.meta.get_mut(blob_id) else {
            return false;
        };
        meta.received.insert(index);
        if meta.received.len() >= usize::try_from(meta.cfg.k).unwrap_or(usize::MAX) {
            self.available.insert(*blob_id);
            true
        } else {
            false
        }
    }

    /// Whether the blob is locally readable.
    #[must_use]
    pub fn is_available(&self, blob_id: &BlobId) -> bool {
        self.available.contains(blob_id)
    }
}

fn erasure_available(blob_id: &BlobId, meta: &BlobMeta, store: &dyn BlobStore) -> bool {
    let cfg = &meta.cfg;
    if meta.received.len() < usize::try_from(cfg.k).unwrap_or(usize::MAX) {
        return false;
    }
    let mut present = Vec::new();
    for index in &meta.received {
        if let Ok(Some(data)) = store.get_chunk(blob_id, *index) {
            present.push((*index, data));
        }
    }
    if present.len() < usize::try_from(cfg.k).unwrap_or(usize::MAX) {
        return false;
    }
    decode_shards(&present, cfg, meta.size_bytes as usize).is_ok()
}
