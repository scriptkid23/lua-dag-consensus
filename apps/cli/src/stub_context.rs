//! Minimal [`consensus::HostContext`] for skeleton replay (plan 03b-1 Task 10).

use consensus::{
    Result,
    host_context::HostContext,
    ports::{Clock, DagView, Persistence, RandomnessBeacon, ValidatorSetPort},
};
use types::{
    crypto_types::Hash32,
    dag::CertifiedVertex,
    macros::{MacroCheckpoint, MacroQc},
    micro::MicroQc,
    primitives::{Epoch, Height, Round, ValidatorId},
    slashing::SlashEvidence,
    validator::ValidatorSet,
};

/// Empty DAG for replay.
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

/// Fixed clock at t=0.
#[derive(Debug, Default)]
pub struct FixedClock;

impl Clock for FixedClock {
    fn now_nanos(&self) -> u128 {
        0
    }
}

/// Empty validator set.
#[derive(Debug, Default)]
pub struct EmptyValidatorSet;

impl ValidatorSetPort for EmptyValidatorSet {
    fn set_for(&self, _epoch: Epoch) -> Result<Option<ValidatorSet>> {
        Ok(Some(ValidatorSet {
            epoch: Epoch(0),
            entries: vec![],
            total_stake: types::primitives::StakeWeight(0),
        }))
    }

    fn index_of(&self, _epoch: Epoch, _validator: &ValidatorId) -> Result<Option<u32>> {
        Ok(None)
    }
}

/// Fixed beacon.
#[derive(Debug)]
pub struct FixedBeacon(pub Hash32);

impl RandomnessBeacon for FixedBeacon {
    fn current(&self) -> Result<Hash32> {
        Ok(self.0)
    }
}

/// No-op persistence.
#[derive(Debug, Default)]
pub struct NoopPersistence;

impl Persistence for NoopPersistence {
    fn store_micro_qc(&self, _qc: &MicroQc) -> Result<()> {
        Ok(())
    }

    fn micro_qc_for(&self, _hash: &Hash32) -> Result<Option<MicroQc>> {
        Ok(None)
    }

    fn store_macro_checkpoint(&self, _cp: &MacroCheckpoint) -> Result<()> {
        Ok(())
    }

    fn store_macro_qc(&self, _qc: &MacroQc) -> Result<()> {
        Ok(())
    }

    fn append_slash_evidence(&self, _ev: &SlashEvidence) -> Result<()> {
        Ok(())
    }

    fn macro_checkpoint_at(&self, _height: Height) -> Result<Option<MacroCheckpoint>> {
        Ok(None)
    }

    fn macro_qc_for(&self, _hash: &Hash32) -> Result<Option<MacroQc>> {
        Ok(None)
    }
}

/// Build a static-lifetime stub context for replay.
#[must_use]
pub fn replay_host_context() -> HostContext<'static> {
    static DAG: EmptyDag = EmptyDag;
    static CLOCK: FixedClock = FixedClock;
    static VALSET: EmptyValidatorSet = EmptyValidatorSet;
    static BEACON: FixedBeacon = FixedBeacon(Hash32::zero());
    static PERSIST: NoopPersistence = NoopPersistence;
    static SIGNER: consensus::ports::PanickingSigner = consensus::ports::PanickingSigner;
    static NO_PENDING: consensus::ports::NoPendingBlobs = consensus::ports::NoPendingBlobs;
    HostContext {
        dag: &DAG,
        clock: &CLOCK,
        valset: &VALSET,
        beacon: &BEACON,
        persistence: &PERSIST,
        signer: &SIGNER,
        pending_blobs: &NO_PENDING,
    }
}
