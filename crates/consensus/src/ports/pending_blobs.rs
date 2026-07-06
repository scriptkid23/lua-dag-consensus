//! `PendingBlobSource` port: blobs queued on this node awaiting
//! inclusion in its next vertex proposal (06-04 design §5).

use types::dag::BlobRef;

/// Drains blob references pending local proposal inclusion.
///
/// `vertex_cert` calls [`PendingBlobSource::drain`] exactly once per
/// proposal it builds; drained refs ride in that vertex. Hosts without
/// blob custody plug in [`NoPendingBlobs`].
pub trait PendingBlobSource: Send + Sync {
    /// Pop every queued `BlobRef` in FIFO order.
    fn drain(&self) -> Vec<BlobRef>;
}

/// Stub for tests, sim, and hosts without blob custody.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoPendingBlobs;

impl PendingBlobSource for NoPendingBlobs {
    fn drain(&self) -> Vec<BlobRef> {
        Vec::new()
    }
}
