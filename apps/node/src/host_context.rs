//! Stub [`consensus::HostContext`] for 03b-1 (production wiring in plan 06b).

use consensus::{
    Result,
    host_context::HostContext,
    ports::{DagView, RandomnessBeacon, ValidatorSetPort},
};
use storage::RocksPersistence;
use types::{
    crypto_types::Hash32,
    dag::CertifiedVertex,
    primitives::{Epoch, Round, ValidatorId},
    validator::ValidatorSet,
};

use crate::{signer::DevSigner, timer::TokioClock};

/// Empty DAG — no vertices until L1 ingress (plan 06b).
#[derive(Debug, Default)]
pub struct EmptyDag;

impl DagView for EmptyDag {
    fn vertex(&self, _hash: &Hash32) -> Result<Option<CertifiedVertex>> {
        Ok(None)
    }

    fn vertices_at_round(&self, _round: Round) -> Result<Vec<CertifiedVertex>> {
        Ok(vec![])
    }
}

/// Fixed beacon bytes until ECVRF wiring (plan 03b-2).
#[derive(Debug, Clone)]
pub struct FixedBeacon(pub Hash32);

impl RandomnessBeacon for FixedBeacon {
    fn current(&self) -> Result<Hash32> {
        Ok(self.0)
    }
}

/// Cached validator set behind [`ValidatorSetPort`].
#[derive(Debug, Clone)]
pub struct CachedValidatorSet {
    set: ValidatorSet,
}

impl CachedValidatorSet {
    /// Wrap a loaded set.
    #[must_use]
    pub fn new(set: ValidatorSet) -> Self {
        Self { set }
    }
}

impl ValidatorSetPort for CachedValidatorSet {
    fn set_for(&self, epoch: Epoch) -> Result<Option<ValidatorSet>> {
        if self.set.epoch == epoch {
            Ok(Some(self.set.clone()))
        } else {
            Ok(None)
        }
    }

    fn index_of(&self, epoch: Epoch, validator: &ValidatorId) -> Result<Option<u32>> {
        if self.set.epoch != epoch {
            return Ok(None);
        }
        Ok(self
            .set
            .entries
            .iter()
            .position(|e| &e.id == validator)
            .map(|i| u32::try_from(i).unwrap_or(u32::MAX)))
    }
}

/// Owned port stubs reused across orchestrator steps.
#[derive(Debug)]
pub struct StubHostBundle {
    /// Empty DAG until L1 adapter lands.
    pub dag: EmptyDag,
    /// Process clock.
    pub clock: TokioClock,
    /// Genesis / loaded validator set.
    pub valset: CachedValidatorSet,
    /// Fixed beacon.
    pub beacon: FixedBeacon,
    /// Dev-only local signer (plan 03d).
    pub signer: DevSigner,
}

impl StubHostBundle {
    /// Build stubs for devnet / skeleton startup.
    #[must_use]
    pub fn new(valset: ValidatorSet) -> Self {
        Self {
            dag: EmptyDag,
            clock: TokioClock::new(),
            valset: CachedValidatorSet::new(valset),
            beacon: FixedBeacon(Hash32::zero()),
            signer: DevSigner::load(None).expect("dev signer must load"),
        }
    }
}

/// Assemble a borrowed [`HostContext`] for one `step` call.
#[must_use]
pub fn build_host_context<'a>(
    bundle: &'a StubHostBundle,
    persistence: &'a RocksPersistence,
) -> HostContext<'a> {
    HostContext {
        dag: &bundle.dag,
        clock: &bundle.clock,
        valset: &bundle.valset,
        beacon: &bundle.beacon,
        persistence,
        signer: &bundle.signer,
    }
}
