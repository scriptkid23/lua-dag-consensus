//! L2 stress scenarios: anchor DoS and network partition.

use sim::args::{Args, Scenario};

#[test]
fn happy_path_is_green_under_full_bullshark() {
    let args = Args {
        validators: 4,
        rounds: 16,
        seed: "0x01".into(),
        scenario: Scenario::HappyPath,
    };
    let report = sim::scenarios::dispatch(&args).unwrap();
    assert!(report.safety_ok, "safety failed: {:?}", report.notes);
    assert!(report.liveness_ok, "liveness failed: {:?}", report.notes);
}

#[test]
fn anchor_dos_blocks_liveness() {
    let args = Args {
        validators: 4,
        rounds: 16,
        seed: "0x02".into(),
        scenario: Scenario::AnchorDos,
    };
    let report = sim::scenarios::dispatch(&args).unwrap();
    assert!(report.safety_ok, "safety failed: {:?}", report.notes);
    assert!(
        !report.liveness_ok,
        "anchor withhold should block MicroQc progress: {:?}",
        report.notes
    );
}

#[test]
fn network_partition_recovers_after_heal() {
    let args = Args {
        validators: 4,
        rounds: 20,
        seed: "0x03".into(),
        scenario: Scenario::NetworkPartition,
    };
    let report = sim::scenarios::dispatch(&args).unwrap();
    assert!(report.safety_ok, "safety failed: {:?}", report.notes);
    assert!(
        report.liveness_ok,
        "expected recovery after partition heal: {:?}",
        report.notes
    );
    assert!(
        report
            .notes
            .iter()
            .any(|n| n.starts_with("partition_healed_at_round_")),
        "missing heal note: {:?}",
        report.notes
    );
}
