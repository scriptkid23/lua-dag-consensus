use std::collections::{HashMap, HashSet};

use types::primitives::BlobId;

use crate::blob::store::BlobStore;
use crate::erasure::{decode_shards, ErasureConfig};

#[derive(Debug, Clone)]
enum CustodyKind {
    Sequential { total_chunks: u32 },
    Erasure { cfg: ErasureConfig },
}

#[derive(Debug, Clone)]
struct BlobMeta {
    size_bytes: u64,
    kind: CustodyKind,
    received: HashSet<u32>,
}

/// In-memory custody ledger; completeness is verified against a [`BlobStore`].
#[derive(Debug, Default)]
pub struct CustodyLedger {
    meta: HashMap<BlobId, BlobMeta>,
    available: HashSet<BlobId>,
}

impl CustodyLedger {
    /// Register sequential chunk expectations (07b).
    pub fn register_sequential(&mut self, blob_id: BlobId, total_chunks: u32, size_bytes: u64) {
        self.upsert_meta(
            blob_id,
            size_bytes,
            CustodyKind::Sequential { total_chunks },
        );
    }

    /// Register erasure shard expectations (07c).
    pub fn register_erasure(
        &mut self,
        blob_id: BlobId,
        cfg: ErasureConfig,
        _n_shards: u32,
        size_bytes: u64,
    ) {
        self.upsert_meta(
            blob_id,
            size_bytes,
            CustodyKind::Erasure { cfg },
        );
    }

    /// Back-compat wrapper for sequential registration.
    pub fn register_meta(&mut self, blob_id: BlobId, total_chunks: u32, size_bytes: u64) {
        self.register_sequential(blob_id, total_chunks, size_bytes);
    }

    fn upsert_meta(&mut self, blob_id: BlobId, size_bytes: u64, kind: CustodyKind) {
        self.meta
            .entry(blob_id)
            .and_modify(|m| {
                m.size_bytes = size_bytes;
                m.kind = kind.clone();
            })
            .or_insert(BlobMeta {
                size_bytes,
                kind,
                received: HashSet::new(),
            });
    }

    /// Record that chunk/shard `index` was ingested; returns `true` when the blob
    /// newly transitions to locally available.
    pub fn note_chunk(&mut self, blob_id: &BlobId, index: u32, store: &dyn BlobStore) -> bool {
        if self.available.contains(blob_id) {
            return false;
        }
        let Some(meta) = self.meta.get_mut(blob_id) else {
            return false;
        };
        meta.received.insert(index);
        let available = match &meta.kind {
            CustodyKind::Sequential { total_chunks } => {
                sequential_complete(blob_id, *total_chunks, store)
            }
            CustodyKind::Erasure { cfg, .. } => erasure_available(blob_id, meta, cfg, store),
        };
        if available {
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

fn sequential_complete(blob_id: &BlobId, total_chunks: u32, store: &dyn BlobStore) -> bool {
    (0..total_chunks).all(|i| store.has_chunk(blob_id, i).unwrap_or(false))
}

fn erasure_available(
    blob_id: &BlobId,
    meta: &BlobMeta,
    cfg: &ErasureConfig,
    store: &dyn BlobStore,
) -> bool {
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
