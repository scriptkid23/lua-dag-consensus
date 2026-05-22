//! Integration: Bullshark wave commit emits `BroadcastMicroQc`.

use std::collections::HashMap;
use std::sync::{Mutex, RwLock};

use consensus::{
    Config, HostContext, StateMachine,
    action::Action,
    bullshark::{select_anchor, wave::WaveId},
    ports::{Clock, DagView, Persistence, RandomnessBeacon, ValidatorSetPort},
};
use crypto::hash::{blake3_with_dst, dst};
use types::{
    crypto_types::{BlsAggSig, BlsPubkey, BlsSig, Hash32},
    dag::{CertifiedVertex, Vertex},
    micro::MicroQc,
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

struct TestValset(ValidatorSet);

impl ValidatorSetPort for TestValset {
    fn set_for(&self, epoch: Epoch) -> consensus::Result<Option<ValidatorSet>> {
        if self.0.epoch == epoch {
            Ok(Some(self.0.clone()))
        } else {
            Ok(None)
        }
    }

    fn index_of(&self, epoch: Epoch, validator: &ValidatorId) -> consensus::Result<Option<u32>> {
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

struct FixedBeacon(Hash32);
impl RandomnessBeacon for FixedBeacon {
    fn current(&self) -> consensus::Result<Hash32> {
        Ok(self.0)
    }
}

struct TestClock;
impl Clock for TestClock {
    fn now_nanos(&self) -> u128 {
        0
    }
}

struct NoopPersistence;
impl Persistence for NoopPersistence {
    fn store_micro_qc(&self, _qc: &MicroQc) -> consensus::Result<()> {
        Ok(())
    }
    fn micro_qc_for(&self, _h: &Hash32) -> consensus::Result<Option<MicroQc>> {
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
    fn append_slash_evidence(&self, _ev: &types::slashing::SlashEvidence) -> consensus::Result<()> {
        Ok(())
    }
    fn macro_checkpoint_at(
        &self,
        _height: types::primitives::Height,
    ) -> consensus::Result<Option<types::macros::MacroCheckpoint>> {
        Ok(None)
    }
    fn macro_qc_for(&self, _h: &Hash32) -> consensus::Result<Option<types::macros::MacroQc>> {
        Ok(None)
    }
}

fn validator_id(i: u32) -> ValidatorId {
    let mut id = [0u8; 32];
    id[..4].copy_from_slice(&i.to_be_bytes());
    ValidatorId(id)
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

fn validator_set(n: u32) -> TestValset {
    let mut entries = Vec::new();
    for i in 0..n {
        entries.push(ValidatorEntry {
            id: validator_id(i),
            bls_pubkey: BlsPubkey([0; 48]),
            vrf_pubkey: types::crypto_types::VrfPubkey::zero(),
            stake: StakeWeight(1_000),
            identity: ValidatorIdentity {
                asn: None,
                cloud: None,
                region: None,
            },
        });
    }
    TestValset(ValidatorSet {
        epoch: Epoch(0),
        entries,
        total_stake: StakeWeight(u64::from(n) * 1_000),
    })
}

/// Wave 1 (rounds 4–7): causal history in 0–3, anchor at 4, full supporter
/// window at 5–6 so shortcut commit and BFS closure both reach `2f+1` authors.
fn setup_dag_for_wave1_commit(n: u32, beacon: &FixedBeacon, cfg: &Config) -> HashMapDag {
    let set = validator_set(n).0;
    let anchor_choice = select_anchor(
        WaveId(1),
        &set,
        beacon as &dyn RandomnessBeacon,
        &cfg.leader,
    )
    .unwrap();
    let anchor_proposer = (0..n)
        .find(|i| validator_id(*i) == anchor_choice.author)
        .expect("anchor author in set");

    let dag = HashMapDag::new();
    let mut prev_hash = None::<Hash32>;
    for r in 0..4 {
        let v = build_vertex(
            r,
            u32::try_from(r % u64::from(n)).expect("proposer"),
            prev_hash.into_iter().collect(),
        );
        prev_hash = Some(v.vertex.hash);
        dag.insert(v);
    }

    let anchor = build_vertex(4, anchor_proposer, prev_hash.into_iter().collect());
    let anchor_hash = anchor.vertex.hash;
    dag.insert(anchor);

    let window = u64::from(cfg.bullshark.shortcut_round_count);
    for r in 5..=4 + window {
        for p in 0..n {
            dag.insert(build_vertex(r, p, vec![anchor_hash]));
        }
    }
    dag
}

static TEST_CLOCK: TestClock = TestClock;
static TEST_BEACON: FixedBeacon = FixedBeacon(Hash32([7u8; 32]));
static TEST_PERSIST: NoopPersistence = NoopPersistence;
static TEST_SIGNER: consensus::ports::PanickingSigner = consensus::ports::PanickingSigner;

#[test]
fn certified_vertex_triggers_broadcast_micro_qc_four_validators() {
    let n = 4;
    let cfg = Config::default_table_17_1();
    let dag = setup_dag_for_wave1_commit(n, &TEST_BEACON, &cfg);
    let valset = validator_set(n);
    let ctx = HostContext {
        dag: &dag,
        clock: &TEST_CLOCK,
        valset: &valset,
        beacon: &TEST_BEACON,
        persistence: &TEST_PERSIST,
        signer: &TEST_SIGNER,
    };
    let mut sm = StateMachine::new(cfg.clone(), ValidatorId::default());
    let trigger_round = Round(4 + u64::from(cfg.bullshark.shortcut_round_count));
    let v = DagView::vertices_at_round(&dag, trigger_round)
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let actions = sm
        .step(consensus::event::Event::CertifiedVertexReceived(v), &ctx)
        .unwrap();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::BroadcastMicroQc(_))),
        "expected BroadcastMicroQc, got {actions:?}"
    );
}

#[test]
fn micro_qc_assembled_twice_is_idempotent() {
    let n = 4;
    let cfg = Config::default_table_17_1();
    let dag = setup_dag_for_wave1_commit(n, &TEST_BEACON, &cfg);
    let valset = validator_set(n);
    let ctx = HostContext {
        dag: &dag,
        clock: &TEST_CLOCK,
        valset: &valset,
        beacon: &TEST_BEACON,
        persistence: &TEST_PERSIST,
        signer: &TEST_SIGNER,
    };
    let mut sm = StateMachine::new(cfg.clone(), ValidatorId::default());
    let trigger_round = Round(4 + u64::from(cfg.bullshark.shortcut_round_count));
    let v = DagView::vertices_at_round(&dag, trigger_round)
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let first = sm
        .step(
            consensus::event::Event::CertifiedVertexReceived(v.clone()),
            &ctx,
        )
        .unwrap();
    let qc = first
        .iter()
        .find_map(|a| {
            if let Action::BroadcastMicroQc(q) = a {
                Some(q.clone())
            } else {
                None
            }
        })
        .expect("first step must broadcast");
    let second = sm
        .step(consensus::event::Event::MicroQcAssembled(qc.clone()), &ctx)
        .unwrap();
    assert!(second.is_empty());
    let third = sm
        .step(consensus::event::Event::MicroQcAssembled(qc), &ctx)
        .unwrap();
    assert!(third.is_empty());
}
