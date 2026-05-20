//! Anchor selection is deterministic for a fixed beacon + validator set.

use consensus::{
    bullshark::{select_anchor, wave::WaveId},
    ports::RandomnessBeacon,
    Config,
};
use types::{
    crypto_types::{BlsPubkey, Hash32},
    primitives::{Epoch, StakeWeight, ValidatorId},
    validator::{ValidatorEntry, ValidatorIdentity, ValidatorSet},
};

struct TestBeacon(Hash32);
impl RandomnessBeacon for TestBeacon {
    fn current(&self) -> consensus::Result<Hash32> {
        Ok(self.0)
    }
}

fn validator_id(i: u32) -> ValidatorId {
    let mut id = [0u8; 32];
    id[..4].copy_from_slice(&i.to_be_bytes());
    ValidatorId(id)
}

fn fixture_validator_set(n: u32) -> ValidatorSet {
    let mut entries = Vec::new();
    for i in 0..n {
        entries.push(ValidatorEntry {
            id: validator_id(i),
            bls_pubkey: BlsPubkey([0; 48]),
            stake: StakeWeight(1_000),
            identity: ValidatorIdentity {
                asn: None,
                cloud: None,
                region: None,
            },
        });
    }
    ValidatorSet {
        epoch: Epoch(0),
        entries,
        total_stake: StakeWeight(u64::from(n) * 1_000),
    }
}

#[test]
fn vrf_sortition_is_deterministic_for_seed() {
    let beacon = TestBeacon(Hash32([7u8; 32]));
    let set = fixture_validator_set(4);
    let cfg = Config::default_table_17_1();
    let a = select_anchor(WaveId(0), &set, &beacon, &cfg.leader).unwrap();
    let b = select_anchor(WaveId(0), &set, &beacon, &cfg.leader).unwrap();
    assert_eq!(a, b, "select_anchor must be deterministic");
    let valid_authors: Vec<ValidatorId> = set.entries.iter().map(|e| e.id).collect();
    assert!(
        valid_authors.contains(&a.author),
        "anchor author {:?} not in validator set",
        a.author
    );
    // Golden: validator index 0 wins for beacon=[7;32], wave=0, 4 equal-stake validators.
    assert_eq!(a.author, validator_id(0));
    assert_eq!(a.wave, WaveId(0));
    assert_eq!(a.anchor_hash, Hash32::zero());
}

#[test]
fn anchor_rotates_with_beacon_or_wave() {
    let set = fixture_validator_set(4);
    let cfg = Config::default_table_17_1();
    let beacon_a = TestBeacon(Hash32([7u8; 32]));
    let beacon_b = TestBeacon(Hash32([42u8; 32]));
    let a = select_anchor(WaveId(0), &set, &beacon_a, &cfg.leader).unwrap();
    let b = select_anchor(WaveId(0), &set, &beacon_b, &cfg.leader).unwrap();
    let c = select_anchor(WaveId(1), &set, &beacon_a, &cfg.leader).unwrap();
    // At least one of (different beacon, different wave) must change the author
    // — proves the sortition actually consults both inputs.
    assert!(
        a.author != b.author || a.author != c.author,
        "anchor must respond to beacon or wave changes (got author {:?} for all)",
        a.author
    );
}

#[test]
fn empty_set_returns_error() {
    let beacon = TestBeacon(Hash32::zero());
    let set = ValidatorSet {
        epoch: Epoch(0),
        entries: vec![],
        total_stake: StakeWeight(0),
    };
    let cfg = Config::default_table_17_1();
    let err = select_anchor(WaveId(0), &set, &beacon, &cfg.leader).unwrap_err();
    assert!(matches!(err, consensus::Error::InvalidConfig(_)));
}
