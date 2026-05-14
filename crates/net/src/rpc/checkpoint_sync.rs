//! Late-joining validator fast-sync via macro checkpoints.

use borsh::{BorshDeserialize, BorshSerialize};
use types::{macros::MacroCheckpoint, primitives::Height};

/// Request: "give me macro checkpoints starting at `from`".
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct CheckpointSyncReq {
    /// First height to send.
    pub from: Height,
    /// Hard cap on number of checkpoints returned.
    pub max_count: u32,
}

/// Response: contiguous macro-checkpoint slice.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct CheckpointSyncResp {
    /// Checkpoints in ascending `Height` order.
    pub checkpoints: Vec<MacroCheckpoint>,
}
