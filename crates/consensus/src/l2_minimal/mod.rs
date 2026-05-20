//! Relaxed L2 commit path for sim milestone 03b-1 (replaced by full Bullshark in 03b-2).

mod book;

pub use book::{Book, WaveStatus};

use std::collections::HashSet;

use crypto::hash::{blake3_with_dst, dst};
use smallvec::smallvec;
use types::{
    crypto_types::{BlsAggSig, BlsSig, Hash32},
    dag::CertifiedVertex,
    micro::MicroQc,
    primitives::{Epoch, Round, ValidatorId},
};

use crate::{
    action::Action,
    bullshark::wave::WaveId,
    host_context::HostContext,
    state_machine::Actions,
    Result,
};

/// Deterministic checkpoint hash over a linearized batch of vertices.
#[must_use]
pub fn checkpoint_hash_from_rounds(vertices: &[CertifiedVertex]) -> Hash32 {
    let hashes: Vec<Hash32> = vertices.iter().map(|cv| cv.vertex.hash).collect();
    let bytes = borsh::to_vec(&hashes).expect("Hash32 vec is always borsh-serializable");
    blake3_with_dst(dst::MICRO_QC, &bytes)
}

/// Relaxed commit: wave `w` is ready when anchor round `4w+3` and every round
/// `4w..=4w+3` has at least one vertex in `DagView`.
fn wave_ready_to_commit(w: u64, dag: &dyn crate::ports::DagView) -> Result<bool> {
    let wave = WaveId(w);
    let anchor = wave.last_round();
    if dag.vertices_at_round(anchor)?.is_empty() {
        return Ok(false);
    }
    for r in wave.first_round().0..=wave.last_round().0 {
        if dag.vertices_at_round(Round(r))?.is_empty() {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Deterministic stand-in for BFS linearization: sort by `(round, author)`.
fn linearize_wave_sorted(w: u64, dag: &dyn crate::ports::DagView) -> Result<Vec<CertifiedVertex>> {
    let wave = WaveId(w);
    let mut out = Vec::new();
    for r in wave.first_round().0..=wave.last_round().0 {
        out.extend(dag.vertices_at_round(Round(r))?);
    }
    out.sort_by(|a, b| {
        a.vertex
            .round
            .0
            .cmp(&b.vertex.round.0)
            .then_with(|| a.vertex.author.0.cmp(&b.vertex.author.0))
    });
    Ok(out)
}

fn validators_in_wave(w: u64, dag: &dyn crate::ports::DagView) -> Result<HashSet<ValidatorId>> {
    let linearized = linearize_wave_sorted(w, dag)?;
    Ok(linearized
        .into_iter()
        .map(|cv| cv.vertex.author)
        .collect())
}

fn stake_threshold_met(
    w: u64,
    ctx: &HostContext<'_>,
    validator_count: u32,
) -> Result<bool> {
    let f = (validator_count - 1) / 3;
    let need = 2 * f + 1;
    let authors = validators_in_wave(w, ctx.dag)?;
    Ok(authors.len() >= usize::try_from(need).unwrap_or(usize::MAX))
}

fn build_flat_micro_qc(
    w: u64,
    checkpoint_hash: Hash32,
    ctx: &HostContext<'_>,
) -> Result<MicroQc> {
    let set = ctx
        .valset
        .set_for(Epoch(0))?
        .ok_or_else(|| crate::Error::InvalidConfig("no validator set for epoch 0".into()))?;
    let authors = validators_in_wave(w, ctx.dag)?;
    let mut bitmap = vec![0u8; set.entries.len().div_ceil(8)];
    for (i, entry) in set.entries.iter().enumerate() {
        if authors.contains(&entry.id) {
            let byte = i / 8;
            let bit = i % 8;
            bitmap[byte] |= 1 << bit;
        }
    }
    Ok(MicroQc {
        checkpoint_hash,
        agg: BlsAggSig {
            sig: BlsSig([0xAB; 96]),
            bitmap,
        },
    })
}

fn validator_count(ctx: &HostContext<'_>) -> Result<u32> {
    let set = ctx
        .valset
        .set_for(Epoch(0))?
        .ok_or_else(|| crate::Error::InvalidConfig("no validator set for epoch 0".into()))?;
    u32::try_from(set.entries.len())
        .map_err(|_| crate::Error::InvalidConfig("validator count overflow".into()))
}

/// Handle a certified vertex: relaxed commit + optional `BroadcastMicroQc`.
pub fn on_certified_vertex(
    book: &mut Book,
    cv: CertifiedVertex,
    ctx: &HostContext<'_>,
) -> Result<Actions> {
    let hash = cv.vertex.hash;
    book.seen
        .insert(hash, (cv.vertex.round, cv.vertex.author));
    let w = WaveId::of_round(cv.vertex.round).0;
    if !wave_ready_to_commit(w, ctx.dag)? {
        return Ok(Actions::new());
    }
    let linearized = linearize_wave_sorted(w, ctx.dag)?;
    let checkpoint_hash = checkpoint_hash_from_rounds(&linearized);
    if book.emitted_micro_qc.contains(&checkpoint_hash) {
        return Ok(Actions::new());
    }
    let n = validator_count(ctx)?;
    if !stake_threshold_met(w, ctx, n)? {
        return Ok(Actions::new());
    }
    let qc = build_flat_micro_qc(w, checkpoint_hash, ctx)?;
    book.emitted_micro_qc.insert(checkpoint_hash);
    book.wave_status.insert(w, WaveStatus::Committed);
    Ok(smallvec![Action::BroadcastMicroQc(qc)])
}

/// Peer MicroQc merge — idempotent, no re-broadcast in 03b-1.
pub fn on_micro_qc_assembled(book: &mut Book, qc: MicroQc) -> Result<Actions> {
    if book.emitted_micro_qc.contains(&qc.checkpoint_hash) {
        return Ok(Actions::new());
    }
    book.emitted_micro_qc.insert(qc.checkpoint_hash);
    Ok(Actions::new())
}
