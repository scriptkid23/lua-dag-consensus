//! Mode B: macro proposer fails primary + backup.

use consensus::Config;

use crate::{checker, scenarios::Report, world::World};

/// Run the scenario.
#[must_use]
pub fn run(validators: u32, rounds: u32, seed: [u8; 32]) -> Report {
    let mut world = World::new(validators, seed, Config::default_table_17_1());
    world.run(rounds);
    Report {
        scenario: "mode_b_fallback".into(),
        validators,
        rounds,
        safety_ok: checker::safety::check(&world).is_ok(),
        liveness_ok: checker::liveness::check(&world).is_ok(),
        lock_macro_ok: checker::lock_macro::check(&world).is_ok(),
        notes: vec!["Mode B adversary wiring deferred to plan 03c".into()],
    }
}
