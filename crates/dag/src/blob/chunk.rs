use borsh::{BorshDeserialize, BorshSerialize};
use types::primitives::BlobId;

/// One sequential payload slice gossiped on `blob-chunk`.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct BlobChunk {
    /// Content-addressed blob identifier.
    pub blob_id: BlobId,
    /// Zero-based chunk index within the payload.
    pub index: u32,
    /// Total chunk count for this blob.
    pub total_chunks: u32,
    /// Full payload size in bytes.
    pub size_bytes: u64,
    /// Raw chunk bytes.
    pub data: Vec<u8>,
}

/// Number of fixed-size chunks required for `size_bytes`.
#[must_use]
pub fn chunk_count(size_bytes: u64, chunk_size: u32) -> u32 {
    let cs = u64::from(chunk_size);
    u32::try_from(size_bytes.div_ceil(cs)).unwrap_or(u32::MAX)
}

/// Split `payload` into sequential gossip chunks.
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
            index: u32::try_from(i).expect("index"),
            total_chunks: total,
            size_bytes,
            data: data.to_vec(),
        })
        .collect()
}
