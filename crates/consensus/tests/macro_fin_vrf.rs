//! VRF macro proposer + Ke + timer contract tests (03c-2).

use std::collections::HashMap;

use consensus::{
    Config, StateMachine,
    macro_fin::{
        ProposerSchedule, aggregation::{compute_ke, mode_a_active},
        proposer::macro_sortition_beta,
    },
};
use types::{
    crypto_types::{BlsPubkey, Hash32},
    primitives::{Epoch, Height, StakeWeight, ValidatorId},
    validator::{ValidatorEntry, ValidatorIdentity, ValidatorSet},
};

fn vset(n: u32) -> ValidatorSet {
    let entries = (0..n)
        .map(|i| {
            let mut id = [0u8; 32];
            id[..4].copy_from_slice(&i.to_be_bytes());
            ValidatorEntry {
                id: ValidatorId(id),
                bls_pubkey: BlsPubkey([0; 48]),
                vrf_pubkey: types::crypto_types::VrfPubkey::zero(),
                stake: StakeWeight(1),
                identity: ValidatorIdentity {
                    asn: None,
                    cloud: None,
                    region: None,
                },
            }
        })
        .collect();
    ValidatorSet {
        epoch: Epoch(0),
        entries,
        total_stake: StakeWeight(u64::from(n)),
    }
}

#[test]
fn vrf_sortition_matches_formula() {
    let set = vset(4);
    let beacon = Hash32([0xBE; 32]);
    let reps = HashMap::new();
    let s = ProposerSchedule::vrf_sortition(&beacon, &set, Height(3), &reps);
    let mut scored: Vec<_> = set
        .entries
        .iter()
        .map(|e| {
            let beta = macro_sortition_beta(&beacon, Height(3), &e.id);
            let score = consensus::leader::vrf_sortition::vrf_sortition_score(
                &beta,
                set.total_stake.0,
                e.stake.0,
                1.0,
            );
            (e.id, score)
        })
        .collect();
    scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    assert_eq!(s.primary, scored[0].0);
    assert_eq!(s.backup, scored[1].0);
}

#[test]
fn compute_ke_thresholds() {
    let cfg = Config::default_table_17_1();
    assert_eq!(compute_ke(&cfg, 499).0, 0);
    assert!(!mode_a_active(compute_ke(&cfg, 499)));
    assert!(mode_a_active(compute_ke(&cfg, 500)));
}

#[test]
fn sim_mode_a_dev_forces_ke() {
    let cfg = Config::sim_mode_a_dev();
    assert!(mode_a_active(compute_ke(&cfg, 8)));
}

#[test]
fn state_machine_still_constructs_with_vrf_book() {
    let sm = StateMachine::new(Config::default_table_17_1(), ValidatorId([1; 32]));
    assert_eq!(sm.config().timing.t_macropropose_ms, 4_000);
}
