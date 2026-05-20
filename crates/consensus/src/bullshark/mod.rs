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

pub use anchor::{AnchorChoice, select_anchor};
pub use commit::{CommitDecision, CommitPath, try_commit_wave};
pub use linearize::{
    Linearization, checkpoint_hash_from_linearization, checkpoint_hash_from_rounds,
};
pub use micro_qc::{EmittedSet, MicroQcBuilder};
pub use wave::WaveId;

use std::collections::{HashMap, HashSet};

use smallvec::SmallVec;
use types::{dag::CertifiedVertex, micro::MicroQc, primitives::Epoch, validator::ValidatorSet};

use crate::{
    action::Action, config::Config, error::Result, event::TimerId, host_context::HostContext,
    leader::timeout::TimerScheduler, state_machine::Actions,
};

/// Per-validator Bullshark wave bookkeeping (in-memory only).
#[derive(Debug, Default)]
pub struct WaveBook {
    /// Waves for which this validator already ran the commit + emit path.
    committed_waves: HashSet<u64>,
    /// Slow-path timer allocated per wave awaiting timeout.
    slow_timer_by_wave: HashMap<u64, TimerId>,
    /// Reverse map from timer id to wave (for `TimerFired`).
    timer_to_wave: HashMap<u64, WaveId>,
    /// Monotonic timer id allocator.
    timers: TimerScheduler,
}

impl WaveBook {
    /// Fresh book with no committed waves or pending timers.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// Drive the full Bullshark path on `CertifiedVertexReceived`.
pub fn on_certified_vertex(
    emitted: &mut EmittedSet,
    waves: &mut WaveBook,
    cfg: &Config,
    cv: CertifiedVertex,
    ctx: &HostContext<'_>,
) -> Result<Actions> {
    let current = WaveId::of_round(cv.vertex.round);
    let mut actions = Actions::new();
    for w in 0..=current.0 {
        let wave = WaveId(w);
        if waves.committed_waves.contains(&w) {
            continue;
        }
        merge_actions(
            &mut actions,
            try_emit_for_wave(wave, cfg, waves, emitted, ctx, false)?,
        );
    }
    Ok(actions)
}

/// Peer-merge only — never re-broadcast a checkpoint this validator already emitted.
pub fn on_micro_qc_assembled(emitted: &EmittedSet, qc: MicroQc) -> Result<Actions> {
    let _ = emitted.contains(&qc.checkpoint_hash);
    Ok(Actions::new())
}

/// Slow-path commit retry after the shortcut window timer fires.
pub fn on_timer_fired(
    emitted: &mut EmittedSet,
    waves: &mut WaveBook,
    cfg: &Config,
    id: TimerId,
    ctx: &HostContext<'_>,
) -> Result<Actions> {
    let Some(wave) = waves.timer_to_wave.remove(&id.0) else {
        return Ok(Actions::new());
    };
    waves.slow_timer_by_wave.remove(&wave.0);
    try_emit_for_wave(wave, cfg, waves, emitted, ctx, true)
}

fn merge_actions(dst: &mut Actions, src: Actions) {
    for action in src {
        dst.push(action);
    }
}

fn slow_path_delay_nanos(cfg: &Config) -> u128 {
    u128::from(cfg.bullshark.shortcut_round_count)
        * u128::from(cfg.timing.round_duration_ms)
        * 1_000_000
}

fn anchor_present(
    wave: WaveId,
    cfg: &Config,
    set: &ValidatorSet,
    ctx: &HostContext<'_>,
) -> Result<bool> {
    let choice = select_anchor(wave, set, ctx.beacon, &cfg.leader)?;
    Ok(ctx
        .dag
        .vertices_at_round(wave.first_round())?
        .iter()
        .any(|v| v.vertex.author == choice.author))
}

fn try_emit_for_wave(
    wave: WaveId,
    cfg: &Config,
    waves: &mut WaveBook,
    emitted: &mut EmittedSet,
    ctx: &HostContext<'_>,
    timed_out: bool,
) -> Result<Actions> {
    if waves.committed_waves.contains(&wave.0) {
        return Ok(Actions::new());
    }

    let Some(set) = ctx.valset.set_for(Epoch(0))? else {
        return Ok(Actions::new());
    };

    let Some(decision) = try_commit_wave(wave, cfg, &set, ctx, timed_out)? else {
        if !timed_out
            && !waves.slow_timer_by_wave.contains_key(&wave.0)
            && anchor_present(wave, cfg, &set, ctx)?
        {
            let id = waves.timers.allocate();
            waves.slow_timer_by_wave.insert(wave.0, id);
            waves.timer_to_wave.insert(id.0, wave);
            return Ok(SmallVec::from_elem(
                Action::ScheduleTimer {
                    id,
                    delay_nanos: slow_path_delay_nanos(cfg),
                },
                1,
            ));
        }
        return Ok(Actions::new());
    };

    waves.committed_waves.insert(wave.0);
    let mut actions = Actions::new();

    if let Some(timer_id) = waves.slow_timer_by_wave.remove(&wave.0) {
        waves.timer_to_wave.remove(&timer_id.0);
        actions.push(Action::CancelTimer(timer_id));
    }

    let lin = Linearization::closure_of_anchor(decision.anchor_hash, ctx.dag)?;
    let checkpoint = checkpoint_hash_from_linearization(&lin);

    if emitted.contains(&checkpoint) {
        return Ok(actions);
    }

    let mut linearized = Vec::with_capacity(lin.order.len());
    for h in &lin.order {
        if let Some(cv) = ctx.dag.vertex(h)? {
            linearized.push(cv);
        }
    }

    let Some(qc) = micro_qc::try_finalize(checkpoint, &linearized, ctx)? else {
        return Ok(actions);
    };

    emitted.insert(checkpoint);
    actions.push(Action::BroadcastMicroQc(qc));
    Ok(actions)
}
