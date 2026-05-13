//! Opaque references into the availability layer.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{crypto_types::Hash32, primitives::BlobId};

/// Reference to a blob in the availability layer.
#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    Hash,
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
)]
pub struct BlobRef {
    /// Blob identifier (content-addressed).
    pub blob_id: BlobId,
    /// KZG / RS commitment root (opaque to consensus).
    pub commitment: Hash32,
    /// Total blob size in bytes.
    pub size_bytes: u64,
}

/// Reference to a single erasure-coded chunk within a blob.
#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    Hash,
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
)]
pub struct ChunkRef {
    /// Parent blob.
    pub blob_id: BlobId,
    /// Index of this chunk within the erasure-coded row/column grid.
    pub index: u32,
}
