//! Bullshark commit: shortcut path with full anchor support, slow path
//! guarded by a `slow_path_round_count` timer.

use std::collections::HashMap;
use std::sync::{Mutex, RwLock};

use consensus::{
    bullshark::{
        commit::{try_commit_wave, CommitPath},
        select_anchor,
        wave::WaveId,
    },
    ports::{DagView, RandomnessBeacon},
    Config, HostContext,
};
use crypto::hash::{blake3_with_dst, dst};
use types::{
    crypto_types::{BlsAggSig, BlsPubkey, BlsSig, Hash32},
    dag::{CertifiedVertex, Vertex},
    primitives::{Epoch, Round, StakeWeight, ValidatorId},
    validator::{ValidatorEntry, ValidatorIdentity, ValidatorSet},
};

struct HashMapDag {
    by_hash: RwLock<HashMap<Hash32, CertifiedVertex>>,
    by_round: Mutex<HashMap<Round, Vec<CertifiedVertex>>>,
}

impl HashMapDag {
    fn new() -> Self {
        Self {
            by_hash: RwLock::new(HashMap::new()),
            by_round: Mutex::new(HashMap::new()),
        }
    }

    fn insert(&self, v: CertifiedVertex) {
        let hash = v.vertex.hash;
        let round = v.vertex.round;
        self.by_round
            .lock()
            .unwrap()
            .entry(round)
            .or_default()
            .push(v.clone());
        self.by_hash.write().unwrap().insert(hash, v);
    }
}

impl DagView for HashMapDag {
    fn vertex(&self, hash: &Hash32) -> consensus::Result<Option<CertifiedVertex>> {
        Ok(self.by_hash.read().unwrap().get(hash).cloned())
    }

    fn vertices_at_round(&self, round: Round) -> consensus::Result<Vec<CertifiedVertex>> {
        Ok(self
            .by_round
            .lock()
            .unwrap()
            .get(&round)
            .cloned()
            .unwrap_or_default())
    }
}

struct FixedBeacon(Hash32);
impl RandomnessBeacon for FixedBeacon {
    fn current(&self) -> consensus::Result<Hash32> {
        Ok(self.0)
    }
}

struct TestClock;
impl consensus::ports::Clock for TestClock {
    fn now_nanos(&self) -> u128 {
        0
    }
}

struct TestValset(ValidatorSet);
impl consensus::ports::ValidatorSetPort for TestValset {
    fn set_for(&self, epoch: Epoch) -> consensus::Result<Option<ValidatorSet>> {
        if self.0.epoch == epoch {
            Ok(Some(self.0.clone()))
        } else {
            Ok(None)
        }
    }
    fn index_of(
        &self,
        epoch: Epoch,
        validator: &ValidatorId,
    ) -> consensus::Result<Option<u32>> {
        if self.0.epoch != epoch {
            return Ok(None);
        }
        Ok(self
            .0
            .entries
            .iter()
            .position(|e| &e.id == validator)
            .map(|i| u32::try_from(i).unwrap_or(u32::MAX)))
    }
}

struct NoopPersistence;
impl consensus::ports::Persistence for NoopPersistence {
    fn store_micro_qc(&self, _qc: &types::micro::MicroQc) -> consensus::Result<()> {
        Ok(())
    }
    fn micro_qc_for(
        &self,
        _h: &Hash32,
    ) -> consensus::Result<Option<types::micro::MicroQc>> {
        Ok(None)
    }
    fn store_macro_checkpoint(
        &self,
        _cp: &types::macros::MacroCheckpoint,
    ) -> consensus::Result<()> {
        Ok(())
    }
    fn store_macro_qc(&self, _qc: &types::macros::MacroQc) -> consensus::Result<()> {
        Ok(())
    }
    fn append_slash_evidence(
        &self,
        _ev: &types::slashing::SlashEvidence,
    ) -> consensus::Result<()> {
        Ok(())
    }
    fn macro_checkpoint_at(
        &self,
        _h: types::primitives::Height,
    ) -> consensus::Result<Option<types::macros::MacroCheckpoint>> {
        Ok(None)
    }
    fn macro_qc_for(
        &self,
        _h: &Hash32,
    ) -> consensus::Result<Option<types::macros::MacroQc>> {
        Ok(None)
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

fn vertex_hash(round: u64, proposer: u32) -> Hash32 {
    let mut buf = Vec::with_capacity(40);
    buf.extend_from_slice(&round.to_be_bytes());
    buf.extend_from_slice(&proposer.to_be_bytes());
    blake3_with_dst(dst::CONTENT_HASH, &buf)
}

fn build_vertex(round: u64, proposer: u32, parents: Vec<Hash32>) -> CertifiedVertex {
    CertifiedVertex {
        vertex: Vertex {
            round: Round(round),
            author: validator_id(proposer),
            parents,
            blobs: vec![],
            hash: vertex_hash(round, proposer),
        },
        certificate: BlsAggSig {
            sig: BlsSig([0xAB; 96]),
            bitmap: vec![0xFF],
        },
    }
}

/// Seed a DAG: anchor at round 0 from the actual anchor author, plus full
/// rounds 1..=window where every validator's vertex parents the anchor.
fn dag_with_full_window(n: u32, window_rounds: u64, anchor_author: ValidatorId) -> HashMapDag {
    let dag = HashMapDag::new();
    let mut anchor_proposer = 0u32;
    for i in 0..n {
        if validator_id(i) == anchor_author {
            anchor_proposer = i;
            break;
        }
    }
    let anchor = build_vertex(0, anchor_proposer, vec![]);
    let anchor_hash = anchor.vertex.hash;
    dag.insert(anchor);
    for r in 1..=window_rounds {
        for p in 0..n {
            dag.insert(build_vertex(r, p, vec![anchor_hash]));
        }
    }
    dag
}

fn host_context<'a>(
    dag: &'a HashMapDag,
    beacon: &'a FixedBeacon,
    clock: &'a TestClock,
    valset: &'a TestValset,
    persist: &'a NoopPersistence,
) -> HostContext<'a> {
    HostContext {
        dag,
        clock,
        valset,
        beacon: beacon as &dyn RandomnessBeacon,
        persistence: persist,
    }
}

#[test]
fn shortcut_commits_when_window_has_2f_plus_1_supporters() {
    let n = 4;
    let set = fixture_validator_set(n);
    let beacon = FixedBeacon(Hash32([7u8; 32]));
    let clock = TestClock;
    let valset = TestValset(set.clone());
    let persist = NoopPersistence;
    let mut cfg = Config::default_table_17_1();
    cfg.bullshark.shortcut_round_count = 4;
    cfg.timing.round_duration_ms = 1;
    let anchor_choice = select_anchor(
        WaveId(0),
        &set,
        &beacon as &dyn RandomnessBeacon,
        &cfg.leader,
    )
    .unwrap();
    let dag = dag_with_full_window(n, 4, anchor_choice.author);
    let ctx = host_context(&dag, &beacon, &clock, &valset, &persist);
    let decision = try_commit_wave(WaveId(0), &cfg, &set, &ctx, false)
        .unwrap()
        .expect("shortcut should fire when 2f+1 supporters exist");
    assert_eq!(decision.path, CommitPath::Shortcut);
    assert_eq!(decision.wave, WaveId(0));
    let anchor_hash = vertex_hash(
        0,
        (0..n)
            .find(|i| validator_id(*i) == anchor_choice.author)
            .unwrap(),
    );
    assert_eq!(decision.anchor_hash, anchor_hash);
}

#[test]
fn no_commit_when_anchor_missing() {
    let n = 4;
    let set = fixture_validator_set(n);
    let beacon = FixedBeacon(Hash32([7u8; 32]));
    let clock = TestClock;
    let valset = TestValset(set.clone());
    let persist = NoopPersistence;
    let cfg = Config::default_table_17_1();
    // Empty DAG: anchor vertex absent.
    let dag = HashMapDag::new();
    let ctx = host_context(&dag, &beacon, &clock, &valset, &persist);
    let decision = try_commit_wave(WaveId(0), &cfg, &set, &ctx, false).unwrap();
    assert!(decision.is_none());
    let decision_timed = try_commit_wave(WaveId(0), &cfg, &set, &ctx, true).unwrap();
    assert!(decision_timed.is_none());
}

#[test]
fn slow_path_commits_only_after_timeout() {
    let n = 4;
    let set = fixture_validator_set(n);
    let beacon = FixedBeacon(Hash32([7u8; 32]));
    let clock = TestClock;
    let valset = TestValset(set.clone());
    let persist = NoopPersistence;
    let mut cfg = Config::default_table_17_1();
    // Shortcut window of 1 round, slow window of 3 rounds. Anchor support
    // arrives only in round 3 — shortcut never fires; slow path requires
    // `timed_out=true`.
    cfg.bullshark.shortcut_round_count = 1;
    cfg.bullshark.slow_path_round_count = 3;
    cfg.timing.round_duration_ms = 1;

    let anchor_choice = select_anchor(
        WaveId(0),
        &set,
        &beacon as &dyn RandomnessBeacon,
        &cfg.leader,
    )
    .unwrap();
    let anchor_proposer = (0..n)
        .find(|i| validator_id(*i) == anchor_choice.author)
        .unwrap();
    let dag = HashMapDag::new();
    let anchor = build_vertex(0, anchor_proposer, vec![]);
    let anchor_hash = anchor.vertex.hash;
    dag.insert(anchor);
    // Round 1 vertices that do NOT reach the anchor (no parents) — kills shortcut.
    for p in 0..n {
        dag.insert(build_vertex(1, p, vec![]));
    }
    // Round 2 vertices that also do NOT reach the anchor.
    for p in 0..n {
        dag.insert(build_vertex(2, p, vec![]));
    }
    // Round 3 vertices DO reach the anchor.
    for p in 0..n {
        dag.insert(build_vertex(3, p, vec![anchor_hash]));
    }

    let ctx = host_context(&dag, &beacon, &clock, &valset, &persist);

    // Without timeout: shortcut window (round 1 only) has zero supporters.
    let shortcut_attempt = try_commit_wave(WaveId(0), &cfg, &set, &ctx, false).unwrap();
    assert!(
        shortcut_attempt.is_none(),
        "shortcut must NOT fire when window has no supporters"
    );

    // With timeout: slow path widens to rounds 1..=3 and finds 2f+1 supporters.
    let slow = try_commit_wave(WaveId(0), &cfg, &set, &ctx, true)
        .unwrap()
        .expect("slow path should commit once timed out");
    assert_eq!(slow.path, CommitPath::SlowPath);
    assert_eq!(slow.anchor_hash, anchor_hash);
}
