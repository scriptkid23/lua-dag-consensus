//! Integration: full Bullshark wave 0 commit emits `BroadcastMicroQc`.
//!
//! Ignored until plan `2026-05-19-03b2-l2-bullshark-full.md` Task 5 rewires
//! the full Bullshark rules; the body still reflects the 03b-1 relaxed
//! commit and will be rewritten in that task.

use std::collections::HashMap;
use std::sync::{Mutex, RwLock};

use consensus::{
    action::Action, ports::Persistence, Config, HostContext, StateMachine,
};
use consensus::ports::{Clock, DagView, RandomnessBeacon, ValidatorSetPort};
use types::{
    crypto_types::{BlsAggSig, BlsSig, Hash32},
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
    fn append_slash_evidence(
        &self,
        _ev: &types::slashing::SlashEvidence,
    ) -> consensus::Result<()> {
        Ok(())
    }
    fn macro_checkpoint_at(
        &self,
        _height: types::primitives::Height,
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

fn build_vertex(round: u64, proposer: u32) -> CertifiedVertex {
    let author = validator_id(proposer);
    let mut bytes = [0u8; 32];
    bytes[..8].copy_from_slice(&round.to_be_bytes());
    let hash = Hash32(bytes);
    CertifiedVertex {
        vertex: Vertex {
            round: Round(round),
            author,
            parents: vec![],
            blobs: vec![],
            hash,
        },
        certificate: BlsAggSig {
            sig: BlsSig([0xAB; 96]),
            bitmap: vec![0xFF],
        },
    }
}

fn setup_dag_with_full_wave0(n: u32) -> HashMapDag {
    let dag = HashMapDag::new();
    for r in 0..=3u64 {
        let v = build_vertex(r, u32::try_from(r % u64::from(n)).expect("proposer index"));
        dag.insert(v);
    }
    dag
}

fn validator_set(n: u32) -> TestValset {
    let mut entries = Vec::new();
    for i in 0..n {
        entries.push(ValidatorEntry {
            id: validator_id(i),
            bls_pubkey: types::crypto_types::BlsPubkey([0; 48]),
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

static TEST_CLOCK: TestClock = TestClock;
static TEST_BEACON: FixedBeacon = FixedBeacon(Hash32::zero());
static TEST_PERSIST: NoopPersistence = NoopPersistence;

#[test]
#[ignore = "re-enabled after 03b-2 Task 5"]
fn certified_vertex_triggers_broadcast_micro_qc_four_validators() {
    let n = 4;
    let dag = setup_dag_with_full_wave0(n);
    let valset = validator_set(n);
    let ctx = HostContext {
        dag: &dag,
        clock: &TEST_CLOCK,
        valset: &valset,
        beacon: &TEST_BEACON,
        persistence: &TEST_PERSIST,
    };
    let mut sm = StateMachine::new(Config::default_table_17_1());
    let v = DagView::vertices_at_round(&dag, Round(3))
        .unwrap()
        .pop()
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
#[ignore = "re-enabled after 03b-2 Task 5"]
fn micro_qc_assembled_twice_is_idempotent() {
    let n = 4;
    let dag = setup_dag_with_full_wave0(n);
    let valset = validator_set(n);
    let ctx = HostContext {
        dag: &dag,
        clock: &TEST_CLOCK,
        valset: &valset,
        beacon: &TEST_BEACON,
        persistence: &TEST_PERSIST,
    };
    let mut sm = StateMachine::new(Config::default_table_17_1());
    let v = DagView::vertices_at_round(&dag, Round(3))
        .unwrap()
        .pop()
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
