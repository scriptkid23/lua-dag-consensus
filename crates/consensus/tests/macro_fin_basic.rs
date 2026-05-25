//! Focused contract tests for L3 03c-1 public surface. Full E2E in apps/sim.

use borsh::to_vec;
use consensus::{Action, Config, StateMachine, macro_fin::ProposerSchedule};
use types::{
    crypto_types::{BlsPubkey, Hash32},
    macros::MacroCheckpoint,
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
fn state_machine_constructs_with_self_id() {
    let cfg = Config::default_table_17_1();
    let id = ValidatorId([7; 32]);
    let sm = StateMachine::new(cfg.clone(), id);
    assert_eq!(
        sm.config().macro_fin.macro_window_w,
        cfg.macro_fin.macro_window_w
    );
}

#[test]
fn round_robin_proposer_matches_spec_formula() {
    let set = vset(4);
    for h in 0u64..12 {
        let s = ProposerSchedule::round_robin(&set, Height(h));
        let h_idx = usize::try_from(h).expect("height fits usize");
        let expected_primary = set.entries[h_idx % 4].id;
        let expected_backup = set.entries[(h_idx + 1) % 4].id;
        assert_eq!(s.primary, expected_primary, "height {h} primary");
        assert_eq!(s.backup, expected_backup, "height {h} backup");
    }
}

#[test]
fn lock_macro_collision_blocks_second_vote_at_same_height() {
    use consensus::lock_macro::LockMacro;
    let v = ValidatorId([0xAA; 32]);
    let mut lm = LockMacro::new();
    lm.try_lock(v, Height(0), Hash32([1; 32])).unwrap();
    let err = lm
        .try_lock(v, Height(0), Hash32([2; 32]))
        .expect_err("conflicting hash at same height must be rejected");
    assert!(err.contains("conflicting"));
}

#[test]
fn lock_macro_extends_to_higher_height() {
    use consensus::lock_macro::LockMacro;
    let v = ValidatorId([1; 32]);
    let mut lm = LockMacro::new();
    lm.try_lock(v, Height(0), Hash32([1; 32])).unwrap();
    lm.try_lock(v, Height(1), Hash32([2; 32])).unwrap();
    assert_eq!(lm.get(&v), Some((Height(1), Hash32([2; 32]))));
}

#[test]
fn action_persist_macro_checkpoint_roundtrips_borsh() {
    let cp = MacroCheckpoint {
        height: Height(3),
        epoch: Epoch(0),
        parent: Hash32([0; 32]),
        micro_root: Hash32([0xAB; 32]),
        hash: Hash32([0xCD; 32]),
    };
    let a = Action::PersistMacroCheckpoint(cp);
    let bytes = to_vec(&a).expect("serialize");
    let b: Action = borsh::from_slice(&bytes).expect("deserialize");
    assert_eq!(a, b);
}
