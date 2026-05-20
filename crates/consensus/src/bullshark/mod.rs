//! Bullshark micro-ordering (whitepaper §8).
//!
//! Public entry points used by `state_machine::StateMachine::step`:
//!   * [`on_certified_vertex`]
//!   * [`on_micro_qc_assembled`]
//!   * [`on_timer_fired`]
//!
//! Module layout mirrors §8:
//!   * [`wave`]       — wave numbering (4 rounds per wave)
//!   * [`anchor`]     — ECVRF anchor selection
//!   * [`commit`]     — shortcut + slow-path commit rules
//!   * [`linearize`]  — BFS closure linearization
//!   * [`micro_qc`]   — flat MicroQc aggregation + idempotency set

pub mod anchor;
pub mod commit;
pub mod linearize;
pub mod micro_qc;
pub mod wave;

pub use anchor::AnchorChoice;
pub use commit::{CommitDecision, CommitPath};
pub use linearize::{
    checkpoint_hash_from_linearization, checkpoint_hash_from_rounds, Linearization,
};
pub use micro_qc::{EmittedSet, MicroQcBuilder};
pub use wave::WaveId;

use types::{dag::CertifiedVertex, micro::MicroQc};

use crate::{
    config::Config, error::Result, event::TimerId, host_context::HostContext,
    state_machine::Actions,
};

/// Skeleton dispatcher for `CertifiedVertexReceived`. Real implementation
/// lands in Task 5 of plan `2026-05-19-03b2-l2-bullshark-full.md`.
pub fn on_certified_vertex(
    _emitted: &mut EmittedSet,
    _cfg: &Config,
    _cv: CertifiedVertex,
    _ctx: &HostContext<'_>,
) -> Result<Actions> {
    Ok(Actions::new())
}

/// Skeleton dispatcher for `MicroQcAssembled`. Real implementation lands
/// in Task 5; this stub keeps the contract `peer-merge only, no
/// re-broadcast` so callers can wire end-to-end before the commit rule
/// is implemented.
pub fn on_micro_qc_assembled(
    emitted: &mut EmittedSet,
    qc: MicroQc,
) -> Result<Actions> {
    if !emitted.contains(&qc.checkpoint_hash) {
        emitted.insert(qc.checkpoint_hash);
    }
    Ok(Actions::new())
}

/// Skeleton dispatcher for `TimerFired`. Real implementation lands in
/// Task 4 (slow-path commit branch).
pub fn on_timer_fired(
    _emitted: &mut EmittedSet,
    _cfg: &Config,
    _id: TimerId,
    _ctx: &HostContext<'_>,
) -> Result<Actions> {
    Ok(Actions::new())
}
