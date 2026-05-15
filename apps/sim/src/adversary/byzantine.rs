//! Byzantine validator behaviour (equivocate, withhold, surround).

use crate::world::World;

/// Mark validator indexes that misbehave. Skeleton: just records the
/// set; scenarios consult `byzantine.contains(&idx)` before sending
/// messages from that validator.
#[derive(Debug, Default)]
pub struct Byzantine {
    /// Misbehaving validator indexes.
    pub indexes: Vec<u32>,
}

impl Byzantine {
    /// Set the byzantine validator indexes.
    pub fn set(&mut self, idxs: Vec<u32>) {
        self.indexes = idxs;
    }
}

/// Hook for scenario authors. Skeleton no-op.
#[allow(dead_code)]
pub fn inject_equivocation(_world: &mut World, _validator: u32) {
    // TODO(plan 03c): synthesise two conflicting MacroProposals and
    // enqueue them on the virtual bus to opposite halves of the network.
}
