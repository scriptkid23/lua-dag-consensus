//! Equivocation injection scenario.

use consensus::Config;

use crate::{
    adversary::byzantine::inject_equivocation,
    checker,
    scenarios::Report,
    world::World,
};

/// Run the scenario.
#[must_use]
pub fn run(validators: u32, rounds: u32, seed: [u8; 32]) -> Report {
    let mut world = World::new(validators, seed, Config::default_table_17_1());
    let offender = 0;
    inject_equivocation(&mut world, offender);
    world.run(rounds);
    let slash_emitted = world.slash_evidence_count() > 0;
    Report {
        scenario: "equivocation_inject".into(),
        validators,
        rounds,
        safety_ok: checker::safety::check(&world).is_ok(),
        liveness_ok: checker::liveness::check(&world).is_ok(),
        lock_macro_ok: checker::lock_macro::check(&world).is_ok(),
        notes: vec![if slash_emitted {
            "slash_emitted".into()
        } else {
            "no slash evidence".into()
        }],
    }
}
