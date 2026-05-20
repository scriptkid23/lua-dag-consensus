//! Network partition with mid-run heal and recovery.

use consensus::Config;

use crate::{
    adversary::network::NetworkConditions,
    checker,
    scenarios::Report,
    world::World,
};

/// Run the scenario.
#[must_use]
pub fn run(validators: u32, rounds: u32, seed: [u8; 32]) -> Report {
    let cfg = Config::default_table_17_1();
    let mut world = World::new(validators, seed, cfg.clone());
    world.set_network_conditions(NetworkConditions::with_round_jitter(
        cfg.timing.round_duration_ms,
    ));

    let mid = validators / 2;
    let left: Vec<u32> = (0..mid).collect();
    let right: Vec<u32> = (mid..validators).collect();
    world.set_partition(left, right);

    let heal_at = rounds / 2;
    for tick in 0..rounds {
        if tick == heal_at {
            world.heal_partition();
        }
        world.tick_round();
    }

    let liveness_ok = checker::liveness::check(&world).is_ok();
    let mut notes = vec![
        format!("partition_healed_at_round_{heal_at}"),
        "lock_macro_skipped_until_03c".into(),
    ];
    if liveness_ok {
        notes.push("recovered_liveness_after_heal".into());
    } else {
        notes.push("liveness_failed_after_heal".into());
    }

    Report {
        scenario: "network_partition".into(),
        validators,
        rounds,
        safety_ok: checker::safety::check(&world).is_ok(),
        liveness_ok,
        lock_macro_ok: checker::lock_macro::check(&world).is_ok(),
        notes,
    }
}
