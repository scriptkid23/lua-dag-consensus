//! Happy path: all validators honest, perfect network. L3 finality active.

use consensus::Config;

use crate::{adversary::network::NetworkConditions, checker, scenarios::Report, world::World};

/// Minimum rounds needed to observe two macro windows (W=8, `wave_round_count=4`)
/// = 2 * 8 * 4 = 64. We add headroom for the timer-driven slow path.
const DEFAULT_ROUNDS: u32 = 96;

/// Run the scenario.
#[must_use]
pub fn run(validators: u32, rounds: u32, seed: [u8; 32]) -> Report {
    let cfg = Config::default_table_17_1();
    let rounds = rounds.max(DEFAULT_ROUNDS);
    let mut world = World::new(validators, seed, cfg.clone());
    world.set_network_conditions(NetworkConditions::with_round_jitter(
        cfg.timing.round_duration_ms,
    ));
    world.run(rounds);
    Report {
        scenario: "happy_path".into(),
        validators,
        rounds,
        safety_ok: checker::safety::check(&world).is_ok(),
        liveness_ok: checker::liveness::check(&world).is_ok(),
        lock_macro_ok: checker::lock_macro::check(&world).is_ok(),
        notes: vec![
            "l3_finality_active".into(),
            "network_jitter_from_round_duration".into(),
        ],
    }
}
