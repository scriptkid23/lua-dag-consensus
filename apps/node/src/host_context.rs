//! Production [`consensus::HostContext`] wiring (plans 06b-l3 / 06b-L1).

use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result as AnyhowResult};
use consensus::{
    Result,
    host_context::HostContext,
    leader::beacon::chain_beacon,
    ports::{RandomnessBeacon, ValidatorSetPort},
};
use storage::RocksPersistence;
use types::{
    crypto_types::Hash32,
    macros::MacroQc,
    primitives::{Epoch, ValidatorId},
    validator::ValidatorSet,
};

use crate::{
    blob::BlobCustodyHandle,
    devnet_keys::validator_id_from_label,
    live_dag::LiveDag,
    signer::DevSigner,
    timer::TokioClock,
};
use consensus::ports::PendingBlobSource;
use types::dag::BlobRef;

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

/// `PendingBlobSource` over the node's blob custody queue.
///
/// `None` (custody disabled) drains nothing — proposals go out empty,
/// preserving liveness.
#[derive(Clone)]
pub struct CustodyPendingBlobs(Option<BlobCustodyHandle>);

impl std::fmt::Debug for CustodyPendingBlobs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustodyPendingBlobs")
            .field("enabled", &self.0.is_some())
            .finish()
    }
}

impl CustodyPendingBlobs {
    /// Wrap an optional custody handle.
    #[must_use]
    pub fn new(custody: Option<BlobCustodyHandle>) -> Self {
        Self(custody)
    }
}

impl PendingBlobSource for CustodyPendingBlobs {
    fn drain(&self) -> Vec<BlobRef> {
        self.0
            .as_ref()
            .map(BlobCustodyHandle::drain_pending)
            .unwrap_or_default()
    }

    fn confirm_attached(&self, blobs: &[BlobRef]) {
        let Some(handle) = self.0.as_ref() else {
            return;
        };
        for blob in blobs {
            let _ = handle.mark_attached(blob.blob_id);
        }
    }
}

/// Owned host ports reused across orchestrator steps.
#[derive(Debug)]
pub struct StubHostBundle {
    /// Live L1 DAG (gossip ingress + Rocks vertex column).
    pub dag: Arc<LiveDag>,
    /// Process clock.
    pub clock: TokioClock,
    /// Genesis / loaded validator set.
    pub valset: CachedValidatorSet,
    /// Macro-QC-chained beacon (shared with `ActionApplier`).
    pub beacon: Arc<ChainedBeacon>,
    /// Dev-only local signer (plan 03d / 06b-l3 pubkey match).
    pub signer: DevSigner,
    /// Pending-blob source for own-vertex proposals (06-04).
    pub pending_blobs: CustodyPendingBlobs,
}

impl StubHostBundle {
    /// Build host ports for devnet startup; signer must match `label` in valset.
    pub fn new(
        label: &str,
        valset: ValidatorSet,
        dag: Arc<LiveDag>,
        signer_key_path: Option<&Path>,
        blob_custody: Option<BlobCustodyHandle>,
    ) -> AnyhowResult<Self> {
        let self_id = validator_id_from_label(label);
        let bls_pubkey = valset
            .entries
            .iter()
            .find(|e| e.id == self_id)
            .with_context(|| format!("self_id {self_id} not found in validator set"))?
            .bls_pubkey;
        let beacon = Arc::new(ChainedBeacon::new());
        Ok(Self {
            dag,
            clock: TokioClock::new(),
            valset: CachedValidatorSet::new(valset),
            beacon,
            signer: DevSigner::load_for_label(label, &bls_pubkey, signer_key_path)?,
            pending_blobs: CustodyPendingBlobs::new(blob_custody),
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
        dag: &*bundle.dag,
        clock: &bundle.clock,
        valset: &bundle.valset,
        beacon: &*bundle.beacon,
        persistence,
        signer: &bundle.signer,
        pending_blobs: &bundle.pending_blobs,
    }
}
