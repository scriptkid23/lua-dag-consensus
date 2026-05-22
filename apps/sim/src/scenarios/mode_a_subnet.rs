//! Mode A subnet aggregation with dev-only config (`sim_mode_a_dev`).

use consensus::Config;

use crate::{checker, scenarios::Report, world::World};

/// Mode A needs more waves than flat Mode 0 for the first `Finalized` blob.
const DEFAULT_ROUNDS: u32 = 128;

/// Run the scenario.
#[must_use]
pub fn run(validators: u32, rounds: u32, seed: [u8; 32]) -> Report {
    let cfg = Config::sim_mode_a_dev();
    let validators = validators.max(8);
    let rounds = rounds.max(DEFAULT_ROUNDS);
    let mut world = World::new(validators, seed, cfg);
    world.run(rounds);
    Report {
        scenario: "mode_a_subnet".into(),
        validators,
        rounds,
        safety_ok: checker::safety::check(&world).is_ok(),
        liveness_ok: checker::liveness::check(&world).is_ok(),
        lock_macro_ok: checker::lock_macro::check(&world).is_ok(),
        notes: vec![
            "l3_mode_a_active".into(),
            "sim_mode_a_dev_threshold".into(),
        ],
    }
}
