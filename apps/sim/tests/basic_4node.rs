//! End-to-end smoke: 4-node `happy_path` with deterministic replay.

use sim::args::{Args, Scenario};

#[test]
fn happy_path_runs_and_replays_bit_identical() {
    let args = Args {
        validators: 4,
        rounds: 16,
        seed: "0x01".into(),
        scenario: Scenario::HappyPath,
    };
    let report = sim::scenarios::dispatch(&args).unwrap();
    assert_eq!(report.validators, 4);
    assert_eq!(report.rounds, 96);
    assert!(report.safety_ok);
    assert!(report.liveness_ok);
    sim::replay::assert_deterministic(&args).unwrap();
}
