//! Owns all validator state machines and ticks them deterministically.

use std::sync::Arc;

use consensus::{Config, StateMachine, ports::Clock};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use types::{
    primitives::{Epoch, StakeWeight, ValidatorId},
    validator::{ValidatorEntry, ValidatorIdentity, ValidatorSet},
};

use crate::{
    virtual_beacon::VirtualBeacon, virtual_clock::VirtualClock, virtual_dag::VirtualDag,
    virtual_net::VirtualNet, virtual_persistence::VirtualPersistence,
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
    /// Shared DAG view (placeholder).
    pub dag: Arc<VirtualDag>,
    /// Shared beacon.
    pub beacon: Arc<VirtualBeacon>,
    /// Per-validator persistence.
    pub persistence: Vec<Arc<VirtualPersistence>>,
    /// Validator set port (shared).
    pub valset: Arc<VirtualValidatorSet>,
    /// Deterministic RNG used by adversaries.
    pub rng: ChaCha20Rng,
    /// Active config (shared with all SMs).
    pub config: Config,
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
        }
    }

    /// Advance the world by one micro-round.
    pub fn tick_round(&mut self) {
        // Deliver any due messages, then advance the clock.
        let now = u64::try_from(self.clock.as_ref().now_nanos()).unwrap_or(u64::MAX);
        let due = self.net.drain_due(now);
        for msg in due {
            let idx = msg.recipient as usize;
            if let Some(sm) = self.machines.get_mut(idx) {
                let _ = sm.step(msg.event);
            }
        }
        let round_nanos = self.config.timing.round_duration_ms * 1_000_000;
        self.clock.advance(round_nanos);
    }

    /// Run `rounds` ticks.
    pub fn run(&mut self, rounds: u32) {
        for _ in 0..rounds {
            self.tick_round();
        }
    }
}
