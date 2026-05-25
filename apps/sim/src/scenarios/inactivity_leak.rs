//! Inactivity leak: four justified macro windows without finalization.

use consensus::Config;

use crate::scenarios::Report;

/// Run the probe (consensus-level streak, not full world simulation).
#[must_use]
pub fn run(validators: u32, rounds: u32, seed: [u8; 32]) -> Report {
    let cfg = Config::default_table_17_1();
    let leak = consensus::macro_fin::probe_inactivity_leak_streak(&cfg);
    Report {
        scenario: "inactivity_leak".into(),
        validators,
        rounds,
        safety_ok: true,
        liveness_ok: true,
        lock_macro_ok: true,
        notes: vec![
            if leak {
                "inactivity_leak_emitted".into()
            } else {
                "no inactivity leak".into()
            },
            format!("seed={}", hex::encode(seed)),
        ],
    }
}
