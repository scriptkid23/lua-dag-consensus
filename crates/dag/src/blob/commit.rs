use crypto::hash::{blake3_with_dst, dst};
use types::primitives::BlobId;

/// Content-addressed blob id from raw payload bytes.
#[must_use]
pub fn blob_id_from_payload(payload: &[u8]) -> BlobId {
    BlobId(blake3_with_dst(dst::BLOB_ID, payload).0)
}
