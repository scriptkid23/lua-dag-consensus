//! Mode B: macro proposer fails primary; backup / leaderless path must finalize.

use std::collections::HashMap;

use consensus::{
    Config, macro_fin::ProposerSchedule,
    ports::{RandomnessBeacon, ValidatorSetPort},
};
use types::primitives::{Epoch, Height};

use crate::{checker, scenarios::Report, world::World};

const DEFAULT_ROUNDS: u32 = 128;

/// Run the scenario.
#[must_use]
pub fn run(validators: u32, rounds: u32, seed: [u8; 32]) -> Report {
    let cfg = Config::default_table_17_1();
    let rounds = rounds.max(DEFAULT_ROUNDS);
    let mut world = World::new(validators, seed, cfg);
    let set = world
        .valset
        .set_for(Epoch(0))
        .expect("valset")
        .expect("epoch 0 set");
    let beacon = world.beacon.current().expect("beacon");
    let primary =
        ProposerSchedule::vrf_sortition(&beacon, &set, Height(0), &HashMap::new()).primary;
    world.suppress_macro_proposals_from(primary);
    world.run(rounds);
    Report {
        scenario: "mode_b_fallback".into(),
        validators,
        rounds,
        safety_ok: checker::safety::check(&world).is_ok(),
        liveness_ok: checker::liveness::check(&world).is_ok(),
        lock_macro_ok: checker::lock_macro::check(&world).is_ok(),
        notes: vec![
            "l3_mode_b_fallback_active".into(),
            "primary_proposer_suppressed".into(),
        ],
    }
}
