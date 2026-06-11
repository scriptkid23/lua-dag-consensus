//! Host-provided ports for one [`crate::StateMachine::step`] call.

use crate::ports::{
    Clock, DagView, PendingBlobSource, Persistence, RandomnessBeacon, SignerPort, ValidatorSetPort,
};

/// Borrowed port bundle passed into [`crate::StateMachine::step`].
#[allow(missing_debug_implementations)]
pub struct HostContext<'a> {
    /// Read-only DAG view (L1 certified vertices).
    pub dag: &'a dyn DagView,
    /// Wall-clock for timer scheduling.
    pub clock: &'a dyn Clock,
    /// Active validator set.
    pub valset: &'a dyn ValidatorSetPort,
    /// Randomness / VRF beacon chain.
    pub beacon: &'a dyn RandomnessBeacon,
    /// Finalized artifact store (read-only from SM in 03b-1).
    pub persistence: &'a dyn Persistence,
    /// Local validator signing (BLS + ECVRF).
    pub signer: &'a dyn SignerPort,
    /// Blobs queued locally for the next own-vertex proposal.
    pub pending_blobs: &'a dyn PendingBlobSource,
}
