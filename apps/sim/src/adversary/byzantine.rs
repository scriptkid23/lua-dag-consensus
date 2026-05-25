//! Byzantine validator behaviour (equivocate, withhold, surround).

use consensus::{
    event::Event,
    macro_fin::checkpoint,
    ports::RandomnessBeacon,
};
use types::crypto_types::Hash32;

use crate::world::World;

/// Mark validator indexes that misbehave.
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

/// Build two conflicting macro proposals and deliver them to opposite partitions.
pub fn inject_equivocation(world: &mut World, validator_idx: u32) {
    let n = u32::try_from(world.machines.len()).expect("validator count");
    world.set_partition(0..n / 2, n / 2..n);
    let beacon = world.beacon.current().expect("beacon");
    let height = types::primitives::Height(0);
    let parent = Hash32::zero();
    let cp_a = checkpoint::build(height, types::primitives::Epoch(0), parent, Hash32([0xA1; 32]));
    let cp_b = checkpoint::build(height, types::primitives::Epoch(0), parent, Hash32([0xB2; 32]));
    let proposal_a = world.signed_macro_proposal(validator_idx, cp_a, beacon);
    let proposal_b = world.signed_macro_proposal(validator_idx, cp_b, beacon);
    for idx in 0..n / 2 {
        world.deliver_proposal(
            idx,
            Event::MacroProposalReceived(proposal_a.clone()),
            0,
        );
    }
    for idx in n / 2..n {
        world.deliver_proposal(
            idx,
            Event::MacroProposalReceived(proposal_b.clone()),
            0,
        );
    }
}
