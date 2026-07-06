use borsh::{BorshDeserialize, BorshSerialize};
use types::primitives::BlobId;

/// Wire payload for one blob-chunk gossip message (07c erasure shard).
///
/// Single-variant enum kept for wire extensibility. Note: the legacy
/// `Sequential` variant was removed 2026-07-06; the borsh tag of
/// `Erasure` shifted from 1 to 0 (pre-production wire break, all nodes
/// upgrade together).
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub enum ChunkPayload {
    /// Reed–Solomon erasure shard.
    Erasure {
        /// Shard index (`0..n-1`).
        index: u32,
        /// Total shard count.
        n_shards: u32,
        /// Shard bytes (fixed size per erasure config).
        data: Vec<u8>,
    },
}

/// One erasure shard gossiped on `blob-chunk`.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct BlobChunk {
    /// Content-addressed blob identifier.
    pub blob_id: BlobId,
    /// Full payload size in bytes (before padding).
    pub size_bytes: u64,
    /// Erasure shard body.
    pub payload: ChunkPayload,
}

impl BlobChunk {
    /// Shard index.
    #[must_use]
    pub fn index(&self) -> u32 {
        let ChunkPayload::Erasure { index, .. } = &self.payload;
        *index
    }

    /// Total shard count (`n_shards`).
    #[must_use]
    pub fn unit_count(&self) -> u32 {
        let ChunkPayload::Erasure { n_shards, .. } = &self.payload;
        *n_shards
    }

    /// Raw shard bytes.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        let ChunkPayload::Erasure { data, .. } = &self.payload;
        data
    }
}

/// Build erasure gossip chunks from RS-encoded shards (07c path).
#[must_use]
pub fn erasure_chunks(blob_id: BlobId, size_bytes: u64, shards: &[Vec<u8>]) -> Vec<BlobChunk> {
    let n_shards = u32::try_from(shards.len()).expect("shard count fits u32");
    shards
        .iter()
        .enumerate()
        .map(|(i, data)| BlobChunk {
            blob_id,
            size_bytes,
            payload: ChunkPayload::Erasure {
                index: u32::try_from(i).expect("index"),
                n_shards,
                data: data.clone(),
            },
        })
        .collect()
}
