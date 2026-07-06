//! Owns all validator state machines and ticks them deterministically.

use std::collections::HashSet;
use std::sync::Arc;

use consensus::{
    Config, HostContext, StateMachine,
    action::Action,
    bullshark::{WaveId, select_anchor},
    leader::beacon::chain_beacon,
    ports::{Clock, DagView, Persistence, RandomnessBeacon, SignerPort, ValidatorSetPort},
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
    adversary::network::NetworkConditions, keys::ValidatorKeyRing,
    vertex_factory::build_quorum_vertices_for_round,
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
    /// Macro QC hashes that already advanced the shared beacon (chain once).
    macro_qc_beacon_chained: HashSet<Hash32>,
    /// Drop `MacroProposal` events from this proposer (Mode B adversary).
    pub suppress_macro_proposer: Option<ValidatorId>,
    /// Deterministic BLS/VRF keys aligned with validator indices.
    key_ring: ValidatorKeyRing,
    /// Count of `NotifyInactivityLeak` actions applied across all validators.
    inactivity_leak_emitted: u32,
    /// When true, vertices come from the distributed vertex_cert protocol
    /// (genesis_propose + gossip) instead of the exogenous factory.
    pub distributed_vertices: bool,
}

impl World {
    /// Build a fresh world of `n` validators using `seed` for the RNG.
    #[must_use]
    pub fn new(n: u32, seed: [u8; 32], config: Config) -> Self {
        use crypto::hash::{blake3_with_dst, dst};

        let key_ring = ValidatorKeyRing::from_seed(seed, n);
        let mut machines = Vec::with_capacity(n as usize);
        let mut entries = Vec::with_capacity(n as usize);
        for i in 0..n {
            let mut id = [0u8; 32];
            id[..4].copy_from_slice(&i.to_be_bytes());
            entries.push(ValidatorEntry {
                id: ValidatorId(id),
                bls_pubkey: key_ring.bls_pubkey(i as usize),
                vrf_pubkey: key_ring.vrf_pubkey(i as usize),
                stake: StakeWeight(1_000),
                identity: ValidatorIdentity {
                    asn: None,
                    cloud: None,
                    region: None,
                },
            });
            machines.push(StateMachine::new(config.clone(), ValidatorId(id)));
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
            macro_qc_beacon_chained: HashSet::new(),
            suppress_macro_proposer: None,
            key_ring,
            inactivity_leak_emitted: 0,
            distributed_vertices: false,
        }
    }

    /// Switch to distributed vertex production (06-04). Call before `run`.
    pub fn enable_distributed_vertices(&mut self) {
        self.distributed_vertices = true;
    }

    /// Clone validator `i`'s BLS secret (adversary tests only).
    #[must_use]
    pub fn key_ring_bls_secret(&self, i: usize) -> crypto::bls::SecretKey {
        self.key_ring.bls_secret(i)
    }

    /// Minimum own-proposal round across all machines (distributed mode).
    #[must_use]
    pub fn min_vertex_round(&self) -> u64 {
        self.machines
            .iter()
            .map(consensus::StateMachine::current_vertex_round)
            .min()
            .unwrap_or(0)
    }

    /// Total slash evidence records across all validators' persistence.
    #[must_use]
    pub fn slash_evidence_total(&self) -> usize {
        self.slash_evidence_count()
    }

    /// Suppress macro proposals from `proposer` (sim adversary).
    pub fn suppress_macro_proposals_from(&mut self, proposer: ValidatorId) {
        self.suppress_macro_proposer = Some(proposer);
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

    /// Deliver an event to one validator (adversary helper).
    pub fn deliver_proposal(
        &mut self,
        recipient: u32,
        event: consensus::Event,
        now: u64,
    ) {
        self.step_validator(recipient, event, now);
    }

    /// Build a signed macro proposal for validator index `proposer_idx`.
    pub fn signed_macro_proposal(
        &self,
        proposer_idx: u32,
        checkpoint: types::macros::MacroCheckpoint,
        beacon: Hash32,
    ) -> types::macros::MacroProposal {
        use consensus::macro_fin::{messages, proposer::vrf_alpha};
        use crypto::hash::dst;
        let set = self
            .valset
            .set_for(Epoch(0))
            .ok()
            .flatten()
            .expect("validator set");
        let proposer = set.entries[proposer_idx as usize].id;
        let alpha = vrf_alpha(&beacon, checkpoint.height, &proposer);
        let signer = self.key_ring.signer(proposer_idx as usize);
        let (vrf_proof, _) = signer.vrf_prove(&alpha).expect("vrf prove");
        let msg = messages::proposer_message(&proposer, &checkpoint);
        types::macros::MacroProposal {
            checkpoint,
            proposer,
            vrf_proof,
            proposer_sig: signer.sign_bls(dst::MACRO_PROPOSER_SIG, &msg),
        }
    }

    /// Count slash evidence entries across all validators.
    pub fn slash_evidence_count(&self) -> usize {
        self.persistence.iter().map(|p| p.slash_count()).sum()
    }

    /// Count inactivity leak notifications emitted during the run.
    #[must_use]
    pub fn inactivity_leak_count(&self) -> u32 {
        self.inactivity_leak_emitted
    }

    fn apply_actions(&mut self, validator_idx: u32, actions: Actions, now: u64) {
        let n = u32::try_from(self.machines.len()).expect("validator count");
        for action in actions {
            if let Action::BroadcastMacroProposal(ref p) = action {
                if self
                    .suppress_macro_proposer
                    .is_some_and(|id| id == p.proposer)
                {
                    continue;
                }
            }
            match action {
                Action::BroadcastMicroQc(qc) => {
                    self.persistence[validator_idx as usize]
                        .store_micro_qc(&qc)
                        .expect("virtual persistence never fails");
                    self.net.enqueue_from_action(
                        validator_idx,
                        &Action::BroadcastMicroQc(qc),
                        n,
                        now,
                        &mut self.rng,
                    );
                }
                Action::BroadcastMacroProposal(p) => {
                    let n_e = n;
                    if consensus::macro_fin::mode_a_active(consensus::macro_fin::compute_ke(
                        &self.config,
                        n_e,
                    )) {
                        // Gossip skips the sender; proposer must still emit subnet partials.
                        self.step_validator(
                            validator_idx,
                            consensus::Event::MacroProposalReceived(p.clone()),
                            now,
                        );
                    }
                    self.net.enqueue_from_action(
                        validator_idx,
                        &Action::BroadcastMacroProposal(p),
                        n,
                        now,
                        &mut self.rng,
                    );
                }
                Action::BroadcastBlsPartial(bp) => {
                    self.net.enqueue_from_action(
                        validator_idx,
                        &Action::BroadcastBlsPartial(bp),
                        n,
                        now,
                        &mut self.rng,
                    );
                }
                Action::BroadcastMacroQc(qc) => {
                    self.persistence[validator_idx as usize]
                        .store_macro_qc(&qc)
                        .expect("virtual persistence never fails");
                    self.net.enqueue_from_action(
                        validator_idx,
                        &Action::BroadcastMacroQc(qc),
                        n,
                        now,
                        &mut self.rng,
                    );
                }
                Action::PersistMacroCheckpoint(cp) => {
                    self.persistence[validator_idx as usize]
                        .store_macro_checkpoint(&cp)
                        .expect("virtual persistence never fails");
                }
                Action::PersistMacroQc(qc) => {
                    self.persistence[validator_idx as usize]
                        .store_macro_qc(&qc)
                        .expect("virtual persistence never fails");
                    if self.macro_qc_beacon_chained.insert(qc.checkpoint_hash) {
                        let prev = self.beacon.current().expect("beacon read");
                        let next = chain_beacon(&prev, &qc.checkpoint_hash);
                        self.beacon.set(next);
                    }
                }
                Action::UpdateBlobStatus { blob, status } => {
                    self.persistence[validator_idx as usize].update_blob_status(blob, status);
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
                Action::BroadcastSubnetAggregate(agg) => {
                    self.net.enqueue_from_action(
                        validator_idx,
                        &Action::BroadcastSubnetAggregate(agg),
                        n,
                        now,
                        &mut self.rng,
                    );
                }
                Action::EmitSlashEvidence { offender, evidence } => {
                    self.persistence[validator_idx as usize]
                        .append_slash_evidence(&evidence)
                        .expect("virtual persistence never fails");
                    self.net.enqueue_from_action(
                        validator_idx,
                        &Action::EmitSlashEvidence {
                            offender,
                            evidence,
                        },
                        n,
                        now,
                        &mut self.rng,
                    );
                }
                Action::NotifyInactivityLeak {
                    windows: _,
                    bps_per_window: _,
                } => {
                    self.inactivity_leak_emitted += 1;
                }
                Action::BroadcastVertexProposal(p) => {
                    self.net.enqueue_from_action(
                        validator_idx,
                        &Action::BroadcastVertexProposal(p),
                        n,
                        now,
                        &mut self.rng,
                    );
                }
                Action::BroadcastVertexPartial(bp) => {
                    self.net.enqueue_from_action(
                        validator_idx,
                        &Action::BroadcastVertexPartial(bp),
                        n,
                        now,
                        &mut self.rng,
                    );
                }
                Action::BroadcastCertifiedVertex(cv) => {
                    // Shared DAG ingest + self-delivery: gossip skips the
                    // sender, but the proposer must also see its own cert
                    // (mirrors the node orchestrator loopback).
                    self.dag.insert(cv.clone());
                    self.step_validator(
                        validator_idx,
                        consensus::Event::CertifiedVertexReceived(cv.clone()),
                        now,
                    );
                    self.net.enqueue_from_action(
                        validator_idx,
                        &Action::BroadcastCertifiedVertex(cv),
                        n,
                        now,
                        &mut self.rng,
                    );
                }
            }
        }
    }

    fn step_validator(&mut self, validator_idx: u32, event: consensus::Event, now: u64) {
        if let consensus::Event::MacroProposalReceived(ref p) = event {
            if self
                .suppress_macro_proposer
                .is_some_and(|id| id == p.proposer)
            {
                return;
            }
        }
        let idx = validator_idx as usize;
        let signer = self.key_ring.signer(idx);
        let no_pending = consensus::ports::NoPendingBlobs;
        let ctx = HostContext {
            dag: self.dag.as_ref(),
            clock: self.clock.as_ref(),
            valset: self.valset.as_ref(),
            beacon: self.beacon.as_ref(),
            persistence: self.persistence[idx].as_ref(),
            signer: &signer,
            pending_blobs: &no_pending,
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
        let set = self
            .valset
            .set_for(Epoch(0))
            .ok()
            .flatten()
            .expect("validator set for vertex production");
        let batch = build_quorum_vertices_for_round(r, &set, parent, &self.key_ring);

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
        if self.distributed_vertices {
            if self.virtual_round == 0 {
                self.genesis_propose_all(now);
            }
        } else {
            self.produce_vertex_tick(now);
        }
        self.drain_timers_and_apply(now);
        let round_nanos = self.config.timing.round_duration_ms * 1_000_000;
        self.clock.advance(round_nanos);
        self.virtual_round += 1;
    }

    /// Genesis-propose on every machine (distributed mode bootstrap).
    fn genesis_propose_all(&mut self, now: u64) {
        for idx in 0..u32::try_from(self.machines.len()).expect("validator count") {
            let i = idx as usize;
            let signer = self.key_ring.signer(i);
            let no_pending = consensus::ports::NoPendingBlobs;
            let ctx = HostContext {
                dag: self.dag.as_ref(),
                clock: self.clock.as_ref(),
                valset: self.valset.as_ref(),
                beacon: self.beacon.as_ref(),
                persistence: self.persistence[i].as_ref(),
                signer: &signer,
                pending_blobs: &no_pending,
            };
            let actions = self.machines[i]
                .genesis_propose(&ctx)
                .unwrap_or_else(|e| panic!("validator {idx} genesis failed: {e}"));
            self.apply_actions(idx, actions, now);
        }
    }

    /// Run `rounds` ticks.
    pub fn run(&mut self, rounds: u32) {
        for _ in 0..rounds {
            self.tick_round();
        }
    }
}
