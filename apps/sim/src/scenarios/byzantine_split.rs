//! Byzantine split-brain scenario with L3 checkers.

use consensus::Config;

use crate::{checker, scenarios::Report, world::World};

const DEFAULT_ROUNDS: u32 = 96;

/// Run the scenario.
#[must_use]
pub fn run(validators: u32, rounds: u32, seed: [u8; 32]) -> Report {
    let rounds = rounds.max(DEFAULT_ROUNDS);
    let mut world = World::new(validators, seed, Config::default_table_17_1());
    world.run(rounds);
    Report {
        scenario: "byzantine_split".into(),
        validators,
        rounds,
        safety_ok: checker::safety::check(&world).is_ok(),
        liveness_ok: checker::liveness::check(&world).is_ok(),
        lock_macro_ok: checker::lock_macro::check(&world).is_ok(),
        notes: vec!["l3_finality_active".into()],
    }
}
