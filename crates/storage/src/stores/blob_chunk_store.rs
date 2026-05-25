//! `(blob_id, chunk_index) -> borsh { total_chunks, size_bytes, data }`.

use borsh::{BorshDeserialize, BorshSerialize};
use types::primitives::BlobId;

use crate::{
    columns::ColumnFamily,
    db::Database,
    error::{Error, Result},
    keys,
};

#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
struct StoredChunk {
    total_chunks: u32,
    size_bytes: u64,
    data: Vec<u8>,
}

/// Persist one chunk row.
pub fn put(
    db: &Database,
    blob_id: &BlobId,
    index: u32,
    total_chunks: u32,
    size_bytes: u64,
    data: &[u8],
) -> Result<()> {
    let key = keys::blob_chunk(blob_id, index);
    let value = StoredChunk {
        total_chunks,
        size_bytes,
        data: data.to_vec(),
    };
    let bytes = borsh::to_vec(&value).map_err(|e| Error::Codec(e.to_string()))?;
    db.put_raw(ColumnFamily::BlobChunk, &key, &bytes)
}

/// Fetch chunk bytes for `(blob_id, index)`.
pub fn get(db: &Database, blob_id: &BlobId, index: u32) -> Result<Option<Vec<u8>>> {
    let key = keys::blob_chunk(blob_id, index);
    let Some(bytes) = db.get_raw(ColumnFamily::BlobChunk, &key)? else {
        return Ok(None);
    };
    let stored: StoredChunk =
        borsh::from_slice(&bytes).map_err(|e| Error::Codec(e.to_string()))?;
    Ok(Some(stored.data))
}

/// Whether `(blob_id, index)` exists.
pub fn has(db: &Database, blob_id: &BlobId, index: u32) -> Result<bool> {
    let key = keys::blob_chunk(blob_id, index);
    Ok(db.get_raw(ColumnFamily::BlobChunk, &key)?.is_some())
}
