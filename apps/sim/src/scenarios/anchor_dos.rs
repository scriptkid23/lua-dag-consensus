//! Anchor `DoS`: withhold the ECVRF anchor vertex each wave.

use consensus::Config;

use crate::{checker, scenarios::Report, world::World};

/// Run the scenario.
#[must_use]
pub fn run(validators: u32, rounds: u32, seed: [u8; 32]) -> Report {
    let mut world = World::new(validators, seed, Config::default_table_17_1());
    world.enable_anchor_withhold();
    world.run(rounds);
    let liveness_ok = checker::liveness::check(&world).is_ok();
    let mut notes = vec![
        "anchor_vertex_withheld_each_wave".into(),
        "lock_macro_skipped_until_03c".into(),
    ];
    if liveness_ok {
        notes.push("unexpected_liveness_under_anchor_withhold".into());
    } else {
        notes.push("liveness_blocked_as_expected".into());
    }
    Report {
        scenario: "anchor_dos".into(),
        validators,
        rounds,
        safety_ok: checker::safety::check(&world).is_ok(),
        liveness_ok,
        lock_macro_ok: checker::lock_macro::check(&world).is_ok(),
        notes,
    }
}
