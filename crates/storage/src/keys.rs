//! Key encoders.
//!
//! All multi-byte integers are big-endian so `RocksDB` lexicographic
//! ordering matches numeric ordering — critical for prefix scans.

use types::{
    crypto_types::Hash32,
    primitives::{BlobId, Epoch, Height, Round, ValidatorId},
};

/// `(round, author)` — 8 + 32 bytes.
#[must_use]
pub fn vertex(round: Round, author: &ValidatorId) -> [u8; 40] {
    let mut out = [0u8; 40];
    out[..8].copy_from_slice(&round.0.to_be_bytes());
    out[8..].copy_from_slice(author.as_bytes());
    out
}

/// `slot` — 8 bytes.
#[must_use]
pub fn slot(slot: u64) -> [u8; 8] {
    slot.to_be_bytes()
}

/// `height` — 8 bytes.
#[must_use]
pub fn height(h: Height) -> [u8; 8] {
    h.0.to_be_bytes()
}

/// `epoch` — 8 bytes.
#[must_use]
pub fn epoch(e: Epoch) -> [u8; 8] {
    e.0.to_be_bytes()
}

/// 32-byte hash key (e.g. `checkpoint_hash`).
#[must_use]
pub fn hash(h: &Hash32) -> [u8; 32] {
    *h.as_bytes()
}

/// `BlobId` key — 32 bytes.
#[must_use]
pub fn blob_id(id: &BlobId) -> [u8; 32] {
    id.0
}

/// `(blob_id, chunk_index)` — 32 + 4 bytes.
#[must_use]
pub fn blob_chunk(blob_id: &BlobId, index: u32) -> [u8; 36] {
    let mut out = [0u8; 36];
    out[..32].copy_from_slice(&blob_id.0);
    out[32..].copy_from_slice(&index.to_be_bytes());
    out
}

/// `(validator, target_epoch)` — 32 + 8 bytes.
#[must_use]
pub fn votebook(validator: &ValidatorId, target_epoch: Epoch) -> [u8; 40] {
    let mut out = [0u8; 40];
    out[..32].copy_from_slice(validator.as_bytes());
    out[32..].copy_from_slice(&target_epoch.0.to_be_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn big_endian_ordering_matches_numeric_ordering() {
        let a = slot(1);
        let b = slot(2);
        let c = slot(256);
        assert!(a < b);
        assert!(b < c);
    }

    #[test]
    fn vertex_key_round_prefix_groups_by_round() {
        let a = vertex(Round(7), &ValidatorId([0; 32]));
        let b = vertex(Round(7), &ValidatorId([0xFF; 32]));
        let c = vertex(Round(8), &ValidatorId([0; 32]));
        assert_eq!(&a[..8], &b[..8]);
        assert!(b < c);
    }
}
