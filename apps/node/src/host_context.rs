//! Production [`consensus::HostContext`] wiring for L3 (plan 06b-l3).

use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result as AnyhowResult};
use consensus::{
    Result,
    host_context::HostContext,
    leader::beacon::chain_beacon,
    ports::{DagView, RandomnessBeacon, ValidatorSetPort},
};
use storage::RocksPersistence;
use types::{
    crypto_types::Hash32,
    dag::CertifiedVertex,
    macros::MacroQc,
    primitives::{Epoch, Round, ValidatorId},
    validator::ValidatorSet,
};

use crate::{devnet_keys::validator_id_from_label, signer::DevSigner, timer::TokioClock};

/// Empty DAG — no vertices until L1 ingress (plan 06b-L1).
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

/// Beacon chained on each locally persisted macro QC (mirrors sim).
#[derive(Debug)]
pub struct ChainedBeacon {
    current: Mutex<Hash32>,
}

impl ChainedBeacon {
    /// Genesis beacon is zero.
    #[must_use]
    pub fn new() -> Self {
        Self {
            current: Mutex::new(Hash32::zero()),
        }
    }

    /// Advance beacon state after adopting a macro QC.
    pub fn adopt_macro_qc(&self, qc: &MacroQc) {
        let mut guard = self.current.lock().expect("beacon lock poisoned");
        *guard = chain_beacon(&*guard, &qc.checkpoint_hash);
    }
}

impl Default for ChainedBeacon {
    fn default() -> Self {
        Self::new()
    }
}

impl RandomnessBeacon for ChainedBeacon {
    fn current(&self) -> Result<Hash32> {
        Ok(*self.current.lock().expect("beacon lock poisoned"))
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
    /// Macro-QC-chained beacon (shared with `ActionApplier`).
    pub beacon: Arc<ChainedBeacon>,
    /// Dev-only local signer (plan 03d / 06b-l3 pubkey match).
    pub signer: DevSigner,
}

impl StubHostBundle {
    /// Build host ports for devnet startup; signer must match `label` in valset.
    pub fn new(
        label: &str,
        valset: ValidatorSet,
        signer_key_path: Option<&Path>,
    ) -> AnyhowResult<Self> {
        let self_id = validator_id_from_label(label);
        let entry = valset
            .entries
            .iter()
            .find(|e| e.id == self_id)
            .with_context(|| format!("self_id {self_id} not found in validator set"))?;
        let beacon = Arc::new(ChainedBeacon::new());
        Ok(Self {
            dag: EmptyDag,
            clock: TokioClock::new(),
            valset: CachedValidatorSet::new(valset),
            beacon,
            signer: DevSigner::load_for_label(label, &entry.bls_pubkey, signer_key_path)?,
        })
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
        beacon: &*bundle.beacon,
        persistence,
        signer: &bundle.signer,
    }
}
