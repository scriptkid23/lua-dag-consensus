use crypto::hash::{blake3_with_dst, dst};
use types::{crypto_types::Hash32, primitives::BlobId};

/// Content-addressed blob id from raw payload bytes.
#[must_use]
pub fn blob_id_from_payload(payload: &[u8]) -> BlobId {
    BlobId(blake3_with_dst(dst::BLOB_ID, payload).0)
}

/// Payload commitment carried in [`types::dag::BlobRef`] (phase B).
#[must_use]
pub fn blob_commitment(payload: &[u8]) -> Hash32 {
    blake3_with_dst(dst::BLOB_COMMIT, payload)
}
