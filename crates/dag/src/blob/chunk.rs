use borsh::{BorshDeserialize, BorshSerialize};
use types::primitives::BlobId;

/// Wire payload for one blob-chunk gossip message (07b sequential or 07c erasure).
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub enum ChunkPayload {
    /// 07b sequential slice (legacy path when erasure disabled).
    Sequential {
        /// Zero-based chunk index.
        index: u32,
        /// Total chunk count.
        total_chunks: u32,
        /// Raw bytes.
        data: Vec<u8>,
    },
    /// 07c Reed–Solomon erasure shard.
    Erasure {
        /// Shard index (`0..n-1`).
        index: u32,
        /// Total shard count.
        n_shards: u32,
        /// Shard bytes (fixed size per erasure config).
        data: Vec<u8>,
    },
}

/// One blob chunk or erasure shard gossiped on `blob-chunk`.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct BlobChunk {
    /// Content-addressed blob identifier.
    pub blob_id: BlobId,
    /// Full payload size in bytes (before padding).
    pub size_bytes: u64,
    /// Sequential slice or erasure shard body.
    pub payload: ChunkPayload,
}

impl BlobChunk {
    /// Shard/chunk index regardless of payload kind.
    #[must_use]
    pub fn index(&self) -> u32 {
        match &self.payload {
            ChunkPayload::Sequential { index, .. } | ChunkPayload::Erasure { index, .. } => *index,
        }
    }

    /// Total units (`total_chunks` or `n_shards`).
    #[must_use]
    pub fn unit_count(&self) -> u32 {
        match &self.payload {
            ChunkPayload::Sequential { total_chunks, .. } => *total_chunks,
            ChunkPayload::Erasure { n_shards, .. } => *n_shards,
        }
    }

    /// Whether this chunk uses the erasure wire shape.
    #[must_use]
    pub fn is_erasure(&self) -> bool {
        matches!(self.payload, ChunkPayload::Erasure { .. })
    }

    /// Raw chunk/shard bytes.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        match &self.payload {
            ChunkPayload::Sequential { data, .. } | ChunkPayload::Erasure { data, .. } => data,
        }
    }
}

/// Number of fixed-size chunks required for `size_bytes`.
#[must_use]
pub fn chunk_count(size_bytes: u64, chunk_size: u32) -> u32 {
    let cs = u64::from(chunk_size);
    u32::try_from(size_bytes.div_ceil(cs)).unwrap_or(u32::MAX)
}

/// Split `payload` into sequential gossip chunks (07b path).
#[must_use]
pub fn split_payload(payload: &[u8], chunk_size: u32) -> Vec<BlobChunk> {
    let blob_id = super::commit::blob_id_from_payload(payload);
    let size_bytes = u64::try_from(payload.len()).expect("payload fits u64");
    let total = chunk_count(size_bytes, chunk_size);
    payload
        .chunks(chunk_size as usize)
        .enumerate()
        .map(|(i, data)| BlobChunk {
            blob_id,
            size_bytes,
            payload: ChunkPayload::Sequential {
                index: u32::try_from(i).expect("index"),
                total_chunks: total,
                data: data.to_vec(),
            },
        })
        .collect()
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
