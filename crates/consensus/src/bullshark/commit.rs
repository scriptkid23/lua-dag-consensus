//! Bullshark commit rule (shortcut + slow path).
//!
//! Whitepaper §8.2:
//!   * **Shortcut** — anchor commits when at least `2f+1` distinct authors
//!     in the `shortcut_round_count` rounds immediately after the anchor
//!     have a parent-path back to it.
//!   * **Slow path** — if the shortcut window does not cross the
//!     threshold within wall-clock budget, a `slow_path_round_count`
//!     timer fires; we then widen the support window and try again.
//!
//! `try_commit_wave` is publicly callable; the host triggers it from the
//! `CertifiedVertexReceived` path (shortcut) and again on `TimerFired`
//! (slow path, with `timed_out = true`).

use std::collections::{HashSet, VecDeque};

use types::{
    crypto_types::Hash32,
    dag::CertifiedVertex,
    primitives::{Round, ValidatorId},
    validator::ValidatorSet,
};

use super::{anchor::select_anchor, wave::WaveId};
use crate::{config::Config, error::Result, host_context::HostContext, ports::DagView};

/// Which commit path resolved the wave.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitPath {
    /// Anchor was committed via the shortcut window.
    Shortcut,
    /// Anchor was committed via the slow-path window (after timeout).
    SlowPath,
}

/// Result of running the commit rule for one wave.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitDecision {
    /// Wave that produced the decision.
    pub wave: WaveId,
    /// Which path won.
    pub path: CommitPath,
    /// Hash of the committed anchor vertex (used by linearization).
    pub anchor_hash: Hash32,
}

/// Attempt to commit `wave`. Returns `Ok(None)` if not yet committable.
///
/// `timed_out` selects between shortcut (false) and slow-path (true)
/// support windows. The slow-path path is a strict superset of the
/// shortcut path's window.
pub fn try_commit_wave(
    wave: WaveId,
    cfg: &Config,
    set: &ValidatorSet,
    ctx: &HostContext<'_>,
    timed_out: bool,
) -> Result<Option<CommitDecision>> {
    if set.entries.is_empty() {
        return Ok(None);
    }
    let anchor_choice = select_anchor(wave, set, ctx.beacon, &cfg.leader)?;
    let anchor_round = wave.first_round();
    let Some(anchor_vertex) = ctx
        .dag
        .vertices_at_round(anchor_round)?
        .into_iter()
        .find(|v| v.vertex.author == anchor_choice.author)
    else {
        return Ok(None);
    };
    let anchor_hash = anchor_vertex.vertex.hash;

    let n = set.entries.len();
    let f = n.saturating_sub(1) / 3;
    let need = 2 * f + 1;

    let shortcut_supporters = count_supporters(
        &anchor_hash,
        anchor_round,
        u64::from(cfg.bullshark.shortcut_round_count),
        ctx.dag,
    )?;
    if shortcut_supporters.len() >= need {
        return Ok(Some(CommitDecision {
            wave,
            path: CommitPath::Shortcut,
            anchor_hash,
        }));
    }

    if timed_out {
        let slow_supporters = count_supporters(
            &anchor_hash,
            anchor_round,
            u64::from(cfg.bullshark.slow_path_round_count),
            ctx.dag,
        )?;
        if slow_supporters.len() >= need {
            return Ok(Some(CommitDecision {
                wave,
                path: CommitPath::SlowPath,
                anchor_hash,
            }));
        }
    }

    Ok(None)
}

/// Distinct authors of vertices in `(anchor_round, anchor_round + window]`
/// that reach the anchor through their parent chain.
fn count_supporters(
    anchor_hash: &Hash32,
    anchor_round: Round,
    window_rounds: u64,
    dag: &dyn DagView,
) -> Result<HashSet<ValidatorId>> {
    let mut supporters: HashSet<ValidatorId> = HashSet::new();
    for offset in 1..=window_rounds {
        let round = Round(anchor_round.0 + offset);
        for v in dag.vertices_at_round(round)? {
            if vertex_reaches(&v, anchor_hash, dag)? {
                supporters.insert(v.vertex.author);
            }
        }
    }
    Ok(supporters)
}

/// Does `start` transitively reference `target` through any parent chain?
fn vertex_reaches(start: &CertifiedVertex, target: &Hash32, dag: &dyn DagView) -> Result<bool> {
    if &start.vertex.hash == target {
        return Ok(true);
    }
    let mut visited: HashSet<Hash32> = HashSet::new();
    let mut queue: VecDeque<Hash32> = start.vertex.parents.iter().copied().collect();
    for h in &queue {
        visited.insert(*h);
    }
    while let Some(h) = queue.pop_front() {
        if &h == target {
            return Ok(true);
        }
        let Some(v) = dag.vertex(&h)? else {
            continue;
        };
        for p in &v.vertex.parents {
            if visited.insert(*p) {
                queue.push_back(*p);
            }
        }
    }
    Ok(false)
}
