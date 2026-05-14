//! Anchor selection (private VRF sortition).

use types::{crypto_types::Hash32, primitives::ValidatorId};

use super::wave::WaveId;

/// Outcome of anchor selection for one wave.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnchorChoice {
    /// Wave this anchor belongs to.
    pub wave: WaveId,
    /// Author who won the anchor slot.
    pub author: ValidatorId,
    /// Hash of the anchor vertex.
    pub anchor_hash: Hash32,
}
