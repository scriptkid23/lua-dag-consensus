//! Read-only consensus queries surfaced to RPC layer.

use types::{
    crypto_types::Hash32,
    macros::MacroQc,
    primitives::{BlobId, Height, Round},
};

use crate::error::Result;

use super::tier::BlobStatus;

/// Read-only queries on the in-memory state. Implementations are
/// expected to be lock-free / single-threaded — concurrency is a host
/// concern.
pub trait ConsensusQuery: Send + Sync {
    /// Last finalized macro QC.
    fn latest_finalized(&self) -> Result<Option<MacroQc>>;

    /// Height of the most recently committed micro-checkpoint.
    fn micro_head(&self) -> Result<Round>;

    /// Status of a specific blob.
    fn blob_status(&self, blob: &BlobId) -> Result<BlobStatus>;

    /// MacroCheckpoint hash at `height`, if any.
    fn macro_checkpoint_hash(&self, height: Height) -> Result<Option<Hash32>>;
}
