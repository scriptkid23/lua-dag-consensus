//! Owns all validator state machines and ticks them deterministically.

use std::sync::Arc;

use consensus::{
    Config, HostContext, StateMachine,
    action::Action,
    bullshark::{WaveId, select_anchor},
    ports::{Clock, DagView, Persistence, ValidatorSetPort},
    state_machine::Actions,
};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use types::{
    crypto_types::Hash32,
    primitives::{Epoch, Round, StakeWeight, ValidatorId},
    validator::{ValidatorEntry, ValidatorIdentity, ValidatorSet},
};

use crate::{
    adversary::network::NetworkConditions, vertex_factory::build_quorum_vertices_for_round,
    virtual_beacon::VirtualBeacon, virtual_clock::VirtualClock, virtual_dag::VirtualDag,
    virtual_net::VirtualNet, virtual_persistence::VirtualPersistence, virtual_timer::VirtualTimer,
    virtual_validator_set::VirtualValidatorSet,
};

/// One simulation world: N validators, one shared network, one clock.
#[derive(Debug)]
pub struct World {
    /// Per-validator state machines (index = validator index).
    pub machines: Vec<StateMachine>,
    /// Shared bus.
    pub net: VirtualNet,
    /// Shared clock.
    pub clock: Arc<VirtualClock>,
    /// Shared DAG view.
    pub dag: Arc<VirtualDag>,
    /// Shared beacon.
    pub beacon: Arc<VirtualBeacon>,
    /// Per-validator persistence.
    pub persistence: Vec<Arc<VirtualPersistence>>,
    /// Validator set port (shared).
    pub valset: Arc<VirtualValidatorSet>,
    /// Deterministic RNG used by adversaries and net fanout.
    pub rng: ChaCha20Rng,
    /// Active config (shared with all SMs).
    pub config: Config,
    /// Monotonic tick counter; distinct from Bullshark `Round` inside vertices.
    pub virtual_round: u64,
    /// Deterministic timer queue.
    pub timer: VirtualTimer,
    /// When set, anchor-round vertices are never inserted or delivered.
    pub anchor_withhold: bool,
}

impl World {
    /// Build a fresh world of `n` validators using `seed` for the RNG.
    #[must_use]
    pub fn new(n: u32, seed: [u8; 32], config: Config) -> Self {
        use crypto::hash::{blake3_with_dst, dst};
        use types::crypto_types::BlsPubkey;

        let mut machines = Vec::with_capacity(n as usize);
        let mut entries = Vec::with_capacity(n as usize);
        for i in 0..n {
            let mut id = [0u8; 32];
            id[..4].copy_from_slice(&i.to_be_bytes());
            entries.push(ValidatorEntry {
                id: ValidatorId(id),
                bls_pubkey: BlsPubkey([0; 48]),
                stake: StakeWeight(1_000),
                identity: ValidatorIdentity {
                    asn: None,
                    cloud: None,
                    region: None,
                },
            });
            machines.push(StateMachine::new(config.clone()));
        }
        let total = u64::from(n) * 1_000;
        let set = ValidatorSet {
            epoch: Epoch(0),
            entries,
            total_stake: StakeWeight(total),
        };

        let beacon_seed = blake3_with_dst(dst::BEACON, &seed);
        Self {
            machines,
            net: VirtualNet::new(),
            clock: Arc::new(VirtualClock::new()),
            dag: Arc::new(VirtualDag::new()),
            beacon: Arc::new(VirtualBeacon::new(beacon_seed)),
            persistence: (0..n)
                .map(|_| Arc::new(VirtualPersistence::new()))
                .collect(),
            valset: Arc::new(VirtualValidatorSet::new(set)),
            rng: ChaCha20Rng::from_seed(seed),
            config,
            virtual_round: 0,
            timer: VirtualTimer::new(),
            anchor_withhold: false,
        }
    }

    /// Withhold the ECVRF anchor vertex each wave (simulates anchor `DoS`).
    pub fn enable_anchor_withhold(&mut self) {
        self.anchor_withhold = true;
    }

    /// Configure network latency / drop / duplicate behaviour.
    pub fn set_network_conditions(&mut self, conditions: NetworkConditions) {
        self.net.set_conditions(conditions);
    }

    /// Split validators into two partitions; cross-partition gossip blocked.
    pub fn set_partition(
        &mut self,
        left: impl IntoIterator<Item = u32>,
        right: impl IntoIterator<Item = u32>,
    ) {
        self.net.set_partition(left, right);
    }

    /// Resume cross-partition delivery.
    pub fn heal_partition(&mut self) {
        self.net.heal_partition();
    }

    fn apply_actions(&mut self, validator_idx: u32, actions: Actions, now: u64) {
        for action in actions {
            match action {
                Action::BroadcastMicroQc(qc) => {
                    self.persistence[validator_idx as usize]
                        .store_micro_qc(&qc)
                        .expect("virtual persistence never fails");
                    self.net.enqueue_from_action(
                        validator_idx,
                        &Action::BroadcastMicroQc(qc),
                        u32::try_from(self.machines.len()).expect("validator count"),
                        now,
                        &mut self.rng,
                    );
                }
                Action::ScheduleTimer { id, delay_nanos } => {
                    self.timer.schedule(
                        now.saturating_add(
                            u64::try_from(delay_nanos.min(u128::from(u64::MAX)))
                                .expect("delay fits u64"),
                        ),
                        id,
                    );
                }
                Action::CancelTimer(id) => self.timer.cancel(id),
                Action::PersistMacroQc(qc) => {
                    let _ = self.persistence[validator_idx as usize].store_macro_qc(&qc);
                }
                Action::UpdateBlobStatus { .. } => {}
                Action::BroadcastMacroProposal(_)
                | Action::BroadcastBlsPartial(_)
                | Action::BroadcastSubnetAggregate(_)
                | Action::BroadcastMacroQc(_)
                | Action::EmitSlashEvidence { .. } => {
                    debug_assert!(false, "unexpected non-L2 action in 03b-1: {action:?}");
                }
            }
        }
    }

    fn step_validator(&mut self, validator_idx: u32, event: consensus::Event, now: u64) {
        let idx = validator_idx as usize;
        let ctx = HostContext {
            dag: self.dag.as_ref(),
            clock: self.clock.as_ref(),
            valset: self.valset.as_ref(),
            beacon: self.beacon.as_ref(),
            persistence: self.persistence[idx].as_ref(),
        };
        let actions = self.machines[idx]
            .step(event, &ctx)
            .unwrap_or_else(|e| panic!("validator {validator_idx} step failed: {e}"));
        self.apply_actions(validator_idx, actions, now);
    }

    fn drain_net_and_apply(&mut self, now: u64) {
        let due = self.net.drain_due(now);
        for msg in due {
            self.step_validator(msg.recipient, msg.event, now);
        }
    }

    fn anchor_hash_for_wave(&self, wave: WaveId) -> Option<Hash32> {
        let set = self.valset.set_for(Epoch(0)).ok()??;
        let choice = select_anchor(wave, &set, self.beacon.as_ref(), &self.config.leader).ok()?;
        let anchor_round = wave.first_round();
        self.dag
            .vertices_at_round(anchor_round)
            .ok()?
            .into_iter()
            .find(|v| v.vertex.author == choice.author)
            .map(|v| v.vertex.hash)
    }

    fn parent_hash_for_round(&self, round: u64) -> Option<Hash32> {
        if round == 0 {
            return None;
        }
        let wave = WaveId::of_round(Round(round));
        let anchor_round = wave.first_round().0;
        if round > anchor_round {
            if let Some(h) = self.anchor_hash_for_wave(wave) {
                return Some(h);
            }
        }
        let prev = Round(round - 1);
        let mut verts = self.dag.vertices_at_round(prev).unwrap_or_default();
        verts.sort_by_key(|v| v.vertex.hash.0);
        verts.first().map(|v| v.vertex.hash)
    }

    fn produce_vertex_tick(&mut self, now: u64) {
        let n = u32::try_from(self.machines.len()).expect("validator count");
        let r = self.virtual_round;
        let parent = self.parent_hash_for_round(r);
        let batch = build_quorum_vertices_for_round(r, n, parent);

        let withheld_author = if self.anchor_withhold {
            let wave = WaveId::of_round(Round(r));
            if r == wave.first_round().0 {
                let set = self
                    .valset
                    .set_for(Epoch(0))
                    .ok()
                    .flatten()
                    .expect("validator set for anchor withhold");
                select_anchor(wave, &set, self.beacon.as_ref(), &self.config.leader)
                    .ok()
                    .map(|choice| choice.author)
            } else {
                None
            }
        } else {
            None
        };

        let mut delivered = Vec::new();
        for cv in batch {
            if withheld_author.is_some_and(|author| cv.vertex.author == author) {
                continue;
            }
            self.dag.insert(cv.clone());
            delivered.push(cv);
        }
        for cv in delivered {
            for idx in 0..n {
                self.step_validator(
                    idx,
                    consensus::Event::CertifiedVertexReceived(cv.clone()),
                    now,
                );
            }
        }
    }

    fn drain_timers_and_apply(&mut self, now: u64) {
        for id in self.timer.drain_due(now) {
            for idx in 0..u32::try_from(self.machines.len()).expect("validator count") {
                self.step_validator(idx, consensus::Event::TimerFired(id), now);
            }
        }
    }

    /// Advance the world by one micro-round (spec §5.5 order).
    pub fn tick_round(&mut self) {
        let now = u64::try_from(self.clock.as_ref().now_nanos()).unwrap_or(u64::MAX);
        self.drain_net_and_apply(now);
        self.produce_vertex_tick(now);
        self.drain_timers_and_apply(now);
        let round_nanos = self.config.timing.round_duration_ms * 1_000_000;
        self.clock.advance(round_nanos);
        self.virtual_round += 1;
    }

    /// Run `rounds` ticks.
    pub fn run(&mut self, rounds: u32) {
        for _ in 0..rounds {
            self.tick_round();
        }
    }
}
