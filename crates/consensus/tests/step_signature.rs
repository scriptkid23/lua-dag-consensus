//! Smoke test: `StateMachine::step` accepts every `Event` variant.

use consensus::{
    Config, StateMachine,
    event::{BlsPartial, Event, SubnetAggregate, SubnetId, TimerId},
    host_context::HostContext,
    ports::{Clock, DagView, Persistence, RandomnessBeacon, ValidatorSetPort},
};
use types::{
    crypto_types::{BlsAggSig, BlsSig, Hash32, VrfProof},
    dag::{CertifiedVertex, Vertex},
    macros::{AggregationMode, MacroCheckpoint, MacroProposal, MacroQc},
    micro::MicroQc,
    primitives::{Epoch, Height, Round, StakeWeight, ValidatorId},
    slashing::{DoubleVote, SlashEvidence},
    validator::ValidatorSet,
};

struct EmptyDag;
impl DagView for EmptyDag {
    fn vertex(&self, _hash: &Hash32) -> consensus::Result<Option<CertifiedVertex>> {
        Ok(None)
    }
    fn vertices_at_round(&self, _round: Round) -> consensus::Result<Vec<CertifiedVertex>> {
        Ok(vec![])
    }
}

struct FixedBeacon(Hash32);
impl RandomnessBeacon for FixedBeacon {
    fn current(&self) -> consensus::Result<Hash32> {
        Ok(self.0)
    }
}

struct EmptyValset;
impl ValidatorSetPort for EmptyValset {
    fn set_for(&self, _epoch: Epoch) -> consensus::Result<Option<ValidatorSet>> {
        Ok(None)
    }
    fn index_of(&self, _epoch: Epoch, _validator: &ValidatorId) -> consensus::Result<Option<u32>> {
        Ok(None)
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
    fn store_macro_checkpoint(&self, _cp: &MacroCheckpoint) -> consensus::Result<()> {
        Ok(())
    }
    fn store_macro_qc(&self, _qc: &MacroQc) -> consensus::Result<()> {
        Ok(())
    }
    fn append_slash_evidence(&self, _ev: &SlashEvidence) -> consensus::Result<()> {
        Ok(())
    }
    fn macro_checkpoint_at(&self, _height: Height) -> consensus::Result<Option<MacroCheckpoint>> {
        Ok(None)
    }
    fn macro_qc_for(&self, _h: &Hash32) -> consensus::Result<Option<MacroQc>> {
        Ok(None)
    }
}

struct TestClock;
impl Clock for TestClock {
    fn now_nanos(&self) -> u128 {
        0
    }
}

fn test_host_context() -> HostContext<'static> {
    static DAG: EmptyDag = EmptyDag;
    static CLOCK: TestClock = TestClock;
    static VALSET: EmptyValset = EmptyValset;
    static BEACON: FixedBeacon = FixedBeacon(Hash32::zero());
    static PERSIST: NoopPersistence = NoopPersistence;
    HostContext {
        dag: &DAG,
        clock: &CLOCK,
        valset: &VALSET,
        beacon: &BEACON,
        persistence: &PERSIST,
    }
}

fn fixture_certified() -> CertifiedVertex {
    CertifiedVertex {
        vertex: Vertex {
            round: Round(0),
            author: ValidatorId([0; 32]),
            parents: vec![],
            blobs: vec![],
            hash: Hash32::zero(),
        },
        certificate: BlsAggSig {
            sig: BlsSig([0; 96]),
            bitmap: vec![],
        },
    }
}

fn fixture_macro_checkpoint() -> MacroCheckpoint {
    MacroCheckpoint {
        height: Height(0),
        epoch: Epoch(0),
        parent: Hash32::zero(),
        micro_root: Hash32::zero(),
        hash: Hash32::zero(),
    }
}

fn fixture_macro_qc() -> MacroQc {
    MacroQc {
        checkpoint_hash: Hash32::zero(),
        mode: AggregationMode::Mode0Flat,
        agg: BlsAggSig {
            sig: BlsSig([0; 96]),
            bitmap: vec![],
        },
    }
}

#[test]
fn step_returns_empty_for_non_l2_events_with_empty_dag() {
    let mut sm = StateMachine::new(Config::default_table_17_1());
    let ctx = test_host_context();
    let events = [
        Event::CertifiedVertexReceived(fixture_certified()),
        Event::MicroQcAssembled(MicroQc {
            checkpoint_hash: Hash32::zero(),
            agg: BlsAggSig {
                sig: BlsSig([0; 96]),
                bitmap: vec![],
            },
        }),
        Event::MacroProposalReceived(MacroProposal {
            checkpoint: fixture_macro_checkpoint(),
            proposer: ValidatorId([0; 32]),
            vrf_proof: VrfProof([0; 80]),
            proposer_sig: BlsSig([0; 96]),
        }),
        Event::BlsPartialReceived(BlsPartial {
            subnet: SubnetId(0),
            validator: ValidatorId([0; 32]),
            checkpoint_hash: Hash32::zero(),
            sig: BlsSig([0; 96]),
        }),
        Event::SubnetAggregateReceived(SubnetAggregate {
            subnet: SubnetId(0),
            checkpoint_hash: Hash32::zero(),
            agg: BlsAggSig {
                sig: BlsSig([0; 96]),
                bitmap: vec![],
            },
        }),
        Event::MacroQcReceived(fixture_macro_qc()),
        Event::TimerFired(TimerId(0)),
        Event::ValidatorSetUpdated {
            epoch: Epoch(0),
            set: ValidatorSet {
                epoch: Epoch(0),
                entries: vec![],
                total_stake: StakeWeight(0),
            },
        },
        Event::SlashEvidenceFound(SlashEvidence::DoubleVote(DoubleVote {
            validator: ValidatorId([0; 32]),
            target: Epoch(0),
            a_sig: BlsSig([0; 96]),
            b_sig: BlsSig([1; 96]),
        })),
    ];
    for ev in events {
        let actions = sm.step(ev, &ctx).expect("step never errors");
        assert!(actions.is_empty(), "empty dag must emit zero actions");
    }
}

#[test]
fn step_is_total_over_event_enum() {
    let mut sm = StateMachine::new(Config::default_table_17_1());
    let ctx = test_host_context();
    sm.step(Event::TimerFired(TimerId(0)), &ctx).unwrap();
}
