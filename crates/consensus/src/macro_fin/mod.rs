//! L3 macro-finality (whitepaper §9).

pub mod aggregation;
pub mod book;
pub mod checkpoint;
pub mod macro_qc;
pub mod messages;
pub mod proposer;
pub mod timer;
pub mod two_chain;
pub mod verify;
pub mod vote_book;
pub mod window;

pub use aggregation::{compute_ke, mode_a_active, Ke, select_mode};
pub use book::MacroBook;
pub use checkpoint::CheckpointBuilder;
pub use macro_qc::MacroQcAssembler;
pub use proposer::ProposerSchedule;
pub use two_chain::TwoChainRule;
pub use vote_book::VoteBook;
pub use window::MacroWindow;

use std::collections::BTreeSet;

use smallvec::SmallVec;
use types::{
    crypto_types::Hash32,
    macros::{AggregationMode, MacroCheckpoint, MacroProposal, MacroQc},
    primitives::{BlobId, Epoch, Height, ValidatorId},
    validator::ValidatorSet,
};

use crate::{
    action::Action,
    config::Config,
    error::Result,
    event::{BlsPartial, SubnetAggregate, SubnetId, TimerId},
    host_context::HostContext,
    leader::reputation::Reputation,
    slashing::{double_vote, surround},
    state_machine::Actions,
};

use crypto::hash::dst;
use proposer::vrf_alpha;
use types::slashing::SlashEvidence;

use aggregation::{mode_a_subnet::ModeASubnet, subnet::SubnetAssign};
use vote_book::VoteRecord;

/// Active aggregation mode at `height` (runtime Mode B overrides threshold).
fn active_mode(book: &MacroBook, cfg: &Config, set: &ValidatorSet, height: Height) -> AggregationMode {
    if book.mode_b_active.contains(&height) {
        return AggregationMode::ModeBLeaderless;
    }
    let n_e = u32::try_from(set.entries.len()).expect("validator count fits u32");
    if aggregation::mode_a_active(aggregation::compute_ke(cfg, n_e)) {
        AggregationMode::ModeASubnet
    } else {
        AggregationMode::Mode0Flat
    }
}

fn beacon_for_height(
    book: &MacroBook,
    ctx: &HostContext<'_>,
    height: Height,
) -> Result<Hash32> {
    if let Some(b) = book.candidate_beacon.get(&height) {
        return Ok(*b);
    }
    ctx.beacon.current()
}

fn subnet_assign(cfg: &Config, set: &ValidatorSet, beacon: &Hash32) -> SubnetAssign {
    let n_e = u32::try_from(set.entries.len()).expect("validator count fits u32");
    SubnetAssign {
        k_e: aggregation::compute_ke(cfg, n_e),
        r_macro: *beacon,
    }
}

/// Only the lexicographically smallest signer assembles a `MacroQc` (avoids
/// duplicate competing QCs under Mode B leaderless gossip in sim).
fn should_assemble_qc(book: &MacroBook, signers: &BTreeSet<types::primitives::ValidatorId>) -> bool {
    signers.iter().min() == Some(&book.self_id)
}

fn bump_reputation(book: &mut MacroBook, cfg: &Config, signers: &BTreeSet<types::primitives::ValidatorId>) {
    for v in signers {
        let entry = book.reputation.entry(*v).or_insert(Reputation::neutral());
        *entry = entry.updated(cfg, 1.0);
    }
}

fn note_inactivity_leak_on_adoption(
    book: &mut MacroBook,
    cfg: &Config,
    actions: &mut Actions,
    finalized_this_step: bool,
) {
    if finalized_this_step {
        book.unfinalized_windows = 0;
        book.leak_notified = false;
        return;
    }
    book.unfinalized_windows = book.unfinalized_windows.saturating_add(1);
    let (bps, apply) = crate::slashing::inactivity_leak::compute(cfg, book.unfinalized_windows);
    if apply && !book.leak_notified {
        book.leak_notified = true;
        actions.push(Action::NotifyInactivityLeak {
            windows: book.unfinalized_windows,
            bps_per_window: bps,
        });
    }
}

fn finish_macro_qc_adoption(
    book: &mut MacroBook,
    cfg: &Config,
    height: Height,
    candidate: MacroCheckpoint,
    qc: MacroQc,
    signers: &BTreeSet<types::primitives::ValidatorId>,
) -> Actions {
    book.emitted_macro_qc.insert(qc.checkpoint_hash);
    book.two_chain.adopt(candidate.clone());
    book.last_macro_hash = candidate.hash;
    book.next_height = Height(height.0 + 1);
    book.proposal_seen.insert(height);
    book.timers.clear_backup(height);
    book.timers.clear_mode_b(height);
    bump_reputation(book, cfg, signers);

    let mut actions = Actions::new();
    actions.push(Action::BroadcastMacroQc(qc.clone()));
    actions.push(Action::PersistMacroCheckpoint(candidate.clone()));
    actions.push(Action::PersistMacroQc(qc));
    actions.push(Action::UpdateBlobStatus {
        blob: blob_id_of_checkpoint(&candidate),
        status: crate::api::tier::BlobStatus::Justified,
    });
    let finalized_this_step = book.two_chain.newly_finalized_height().is_some();
    note_inactivity_leak_on_adoption(book, cfg, &mut actions, finalized_this_step);
    if let Some(prev) = book.two_chain.newly_finalized_height() {
        book.two_chain.mark_finalized(prev);
        if let Some(prev_cp) = book.candidate.get(&prev).cloned() {
            actions.push(Action::UpdateBlobStatus {
                blob: blob_id_of_checkpoint(&prev_cp),
                status: crate::api::tier::BlobStatus::Finalized,
            });
        }
    }
    actions
}

/// Sim/CLI probe: broken-parent macro streak emits leak after four justified windows.
#[must_use]
pub fn probe_inactivity_leak_streak(cfg: &Config) -> bool {
    let mut book = MacroBook::new(ValidatorId([0; 32]));
    let mut actions = Actions::new();
    for i in 0..4u64 {
        let height = Height(i);
        let parent = if i == 0 {
            Hash32::zero()
        } else {
            Hash32([0x99; 32])
        };
        let cp = checkpoint::build(height, Epoch(0), parent, Hash32([i as u8 + 1; 32]));
        book.candidate.insert(height, cp.clone());
        book.two_chain.adopt(cp);
        let finalized = book.two_chain.newly_finalized_height().is_some();
        note_inactivity_leak_on_adoption(&mut book, cfg, &mut actions, finalized);
        if let Some(prev) = book.two_chain.newly_finalized_height() {
            book.two_chain.mark_finalized(prev);
        }
    }
    actions
        .iter()
        .any(|a| matches!(a, Action::NotifyInactivityLeak { .. }))
}

fn try_emit_mode_a_qc(
    book: &mut MacroBook,
    cfg: &Config,
    checkpoint_hash: Hash32,
    height: Height,
    ctx: &HostContext<'_>,
) -> Result<Actions> {
    let set = ctx
        .valset
        .set_for(Epoch(0))?
        .ok_or_else(|| crate::Error::InvalidConfig("no validator set for epoch 0".into()))?;
    if book.mode_b_active.contains(&height) {
        return Ok(Actions::new());
    }
    let beacon = beacon_for_height(book, ctx, height)?;
    let schedule = ProposerSchedule::vrf_sortition(&beacon, &set, height, &book.reputation);
    if book.self_id != schedule.primary {
        return Ok(Actions::new());
    }
    let aggs = book.subnet_aggs.get(&checkpoint_hash).cloned().unwrap_or_default();
    let Some(qc) = macro_qc::try_finalize_mode_a_from_aggs(
        checkpoint_hash,
        &aggs,
        &set,
        &book.partial_sigs,
    ) else {
        return Ok(Actions::new());
    };
    if book.emitted_macro_qc.contains(&checkpoint_hash) {
        return Ok(Actions::new());
    }
    let candidate = book
        .candidate
        .get(&height)
        .cloned()
        .expect("candidate for subnet finalize");
    let signers: BTreeSet<_> = book
        .partials
        .get(&checkpoint_hash)
        .cloned()
        .unwrap_or_default();
    Ok(finish_macro_qc_adoption(book, cfg, height, candidate, qc, &signers))
}

/// Per-event L3 entry point: called from `StateMachine::step` after bullshark
/// produced `actions`. Scans `actions` for `Action::BroadcastMicroQc` variants
/// and, for each, pushes `qc.checkpoint_hash` into `book.micro_ring`. On the
/// W-th push the round-robin primary emits `BroadcastMacroProposal` for the
/// next macro height.
pub fn on_local_micro_qcs(
    book: &mut MacroBook,
    cfg: &Config,
    ctx: &HostContext<'_>,
    actions: &mut Actions,
) -> Result<()> {
    let new_hashes: SmallVec<[Hash32; 4]> = actions
        .iter()
        .filter_map(|a| match a {
            Action::BroadcastMicroQc(qc) => Some(qc.checkpoint_hash),
            _ => None,
        })
        .collect();

    if new_hashes.is_empty() {
        return Ok(());
    }

    let w = cfg.macro_fin.macro_window_w as usize;
    let set = ctx
        .valset
        .set_for(Epoch(0))?
        .ok_or_else(|| crate::Error::InvalidConfig("no validator set for epoch 0".into()))?;

    for h in new_hashes {
        book.micro_ring.push_back(h);
        if book.micro_ring.len() < w {
            continue;
        }
        let candidate_height = book.next_height;
        let micro_root = book::micro_root_of_ring(&book.micro_ring);
        let candidate = checkpoint::build(
            candidate_height,
            Epoch(0),
            book.last_macro_hash,
            micro_root,
        );
        book.micro_ring.clear();
        book.candidate.insert(candidate_height, candidate.clone());
        let beacon = ctx.beacon.current()?;
        book.candidate_beacon.insert(candidate_height, beacon);

        let schedule =
            ProposerSchedule::vrf_sortition(&beacon, &set, candidate_height, &book.reputation);
        if schedule.primary == book.self_id {
            let alpha = vrf_alpha(&beacon, candidate_height, &book.self_id);
            let (vrf_proof, _) = ctx.signer.vrf_prove(&alpha)?;
            let msg = messages::proposer_message(&book.self_id, &candidate);
            let proposer_sig = ctx.signer.sign_bls(dst::MACRO_PROPOSER_SIG, &msg);
            actions.push(Action::BroadcastMacroProposal(MacroProposal {
                checkpoint: candidate.clone(),
                proposer: book.self_id,
                vrf_proof,
                proposer_sig,
            }));
            book.proposal_seen.insert(candidate_height);
        } else if schedule.backup == book.self_id {
            actions.push(book.timers.backup_propose_action(cfg, candidate_height));
        }
        // Dev Mode-A sim (`sim_force_ke`) relies on subnet path only — no Mode B race.
        if cfg.aggregation.sim_force_ke.is_none() {
            actions.push(book.timers.mode_b_deadline_action(cfg, candidate_height));
        }
    }
    Ok(())
}

/// Handle an inbound macro proposal: verify proposer, lock, emit partial.
pub fn on_macro_proposal(
    book: &mut MacroBook,
    cfg: &Config,
    p: MacroProposal,
    ctx: &HostContext<'_>,
) -> Result<Actions> {
    let set = ctx
        .valset
        .set_for(Epoch(0))?
        .ok_or_else(|| crate::Error::InvalidConfig("no validator set for epoch 0".into()))?;

    let height = p.checkpoint.height;
    if !book.candidate_beacon.contains_key(&height) {
        book.candidate_beacon
            .insert(height, ctx.beacon.current()?);
    }
    let beacon = beacon_for_height(book, ctx, height)?;
    let alpha = vrf_alpha(&beacon, height, &p.proposer);
    if !verify::verify_proposal(&set, &p, &alpha) {
        book.rejected_crypto += 1;
        return Ok(Actions::new());
    }

    let proposer_key = (height, p.proposer);
    if let Some(existing) = book.proposals_seen.get(&proposer_key) {
        if existing
            .iter()
            .any(|prev| prev.checkpoint.hash != p.checkpoint.hash)
        {
            let first = existing[0].clone();
            let mut actions = Actions::new();
            actions.push(Action::EmitSlashEvidence {
                offender: p.proposer,
                evidence: SlashEvidence::MacroEquivocation(crate::slashing::equivocation::detect(
                    p.proposer, first, p,
                )),
            });
            return Ok(actions);
        }
        if existing
            .iter()
            .any(|prev| prev.checkpoint.hash == p.checkpoint.hash)
        {
            return Ok(Actions::new());
        }
    }
    book.proposals_seen
        .entry(proposer_key)
        .or_default()
        .push(p.clone());

    let schedule = ProposerSchedule::vrf_sortition(&beacon, &set, height, &book.reputation);
    let mode = active_mode(book, cfg, &set, height);
    if mode != AggregationMode::ModeBLeaderless
        && p.proposer != schedule.primary
        && p.proposer != schedule.backup
    {
        return Ok(Actions::new());
    }
    if p.checkpoint.parent != book.last_macro_hash {
        return Ok(Actions::new());
    }
    let local = book
        .candidate
        .get(&height)
        .filter(|c| c.micro_root == p.checkpoint.micro_root && c.hash == p.checkpoint.hash);
    if local.is_none() {
        return Ok(Actions::new());
    }

    if book
        .locks
        .try_lock(book.self_id, height, p.checkpoint.hash)
        .is_err()
    {
        book.suppressed_conflicts = book.suppressed_conflicts.saturating_add(1);
        return Ok(Actions::new());
    }

    book.proposal_seen.insert(height);
    book.timers.clear_backup(height);
    book.two_chain.adopt(p.checkpoint.clone());
    book.candidate.insert(height, p.checkpoint.clone());
    book.partials
        .entry(p.checkpoint.hash)
        .or_default()
        .insert(book.self_id);

    let vote = VoteRecord {
        source: Epoch(height.0.saturating_sub(1)),
        target: Epoch(height.0),
        checkpoint: p.checkpoint.hash,
        sig: ctx.signer.sign_bls(
            dst::MACRO_VOTE,
            &messages::vote_message(&VoteRecord {
                source: Epoch(height.0.saturating_sub(1)),
                target: Epoch(height.0),
                checkpoint: p.checkpoint.hash,
                sig: types::crypto_types::BlsSig([0; 96]),
            }),
        ),
    };
    book.votes.record(book.self_id, vote);

    let mut actions = Actions::new();
    if let Some(ev) = surround::scan_for_surround(&book.votes, &book.self_id)? {
        actions.push(Action::EmitSlashEvidence {
            offender: book.self_id,
            evidence: SlashEvidence::Surround(ev),
        });
    }
    if let Some(ev) = double_vote::scan_for_double_vote(&book.votes, &book.self_id)? {
        actions.push(Action::EmitSlashEvidence {
            offender: book.self_id,
            evidence: SlashEvidence::DoubleVote(ev),
        });
    }

    let assign = subnet_assign(cfg, &set, &beacon);
    let subnet = SubnetId(assign.index_for(&book.self_id));
    let checkpoint_msg = messages::checkpoint_message(&p.checkpoint);
    let sig = ctx.signer.sign_bls(dst::MACRO_CHECKPOINT, &checkpoint_msg);
    book.partial_sigs
        .insert((p.checkpoint.hash, book.self_id), sig);
    actions.push(Action::BroadcastBlsPartial(BlsPartial {
        subnet: if mode == AggregationMode::ModeASubnet {
            subnet
        } else {
            SubnetId(0)
        },
        validator: book.self_id,
        checkpoint_hash: p.checkpoint.hash,
        sig,
    }));
    actions.push(Action::UpdateBlobStatus {
        blob: blob_id_of_checkpoint(&p.checkpoint),
        status: crate::api::tier::BlobStatus::SoftConfirmed,
    });

    if mode == AggregationMode::ModeASubnet {
        let signers = book
            .subnet_partials
            .entry((p.checkpoint.hash, subnet))
            .or_default();
        signers.insert(book.self_id);
        if let Some(agg) = ModeASubnet::try_build_aggregate(
            p.checkpoint.hash,
            subnet,
            signers,
            &set,
            &assign,
            &book.partial_sigs,
        ) {
            if ModeASubnet::aggregator_for(subnet, &set, &assign) == Some(book.self_id) {
                actions.push(Action::BroadcastSubnetAggregate(agg));
            }
        }
    }

    Ok(actions)
}

/// Aggregate partials into a MacroQc once threshold is met.
pub fn on_bls_partial(
    book: &mut MacroBook,
    cfg: &Config,
    bp: BlsPartial,
    ctx: &HostContext<'_>,
) -> Result<Actions> {
    let Some(height) = book
        .candidate
        .iter()
        .find(|(_, c)| c.hash == bp.checkpoint_hash)
        .map(|(h, _)| *h)
    else {
        return Ok(Actions::new());
    };

    let set = ctx
        .valset
        .set_for(Epoch(0))?
        .ok_or_else(|| crate::Error::InvalidConfig("no validator set for epoch 0".into()))?;
    let checkpoint = book
        .candidate
        .get(&height)
        .cloned()
        .expect("checkpoint for partial");

    if !verify::verify_partial(&set, &bp, &checkpoint) {
        book.rejected_crypto += 1;
        return Ok(Actions::new());
    }
    book.partial_sigs
        .insert((bp.checkpoint_hash, bp.validator), bp.sig);

    if book.emitted_macro_qc.contains(&bp.checkpoint_hash) {
        return Ok(Actions::new());
    }

    let mode = active_mode(book, cfg, &set, height);
    let beacon = beacon_for_height(book, ctx, height)?;
    let assign = subnet_assign(cfg, &set, &beacon);

    match mode {
        AggregationMode::ModeASubnet => {
            let subnet = bp.subnet;
            let signers = book
                .subnet_partials
                .entry((bp.checkpoint_hash, subnet))
                .or_default();
            signers.insert(bp.validator);
            book.partials.entry(bp.checkpoint_hash).or_default().insert(bp.validator);

            let mut actions = Actions::new();
            if let Some(agg) = ModeASubnet::try_build_aggregate(
                bp.checkpoint_hash,
                subnet,
                signers,
                &set,
                &assign,
                &book.partial_sigs,
            ) {
                if ModeASubnet::aggregator_for(subnet, &set, &assign) == Some(book.self_id) {
                    actions.push(Action::BroadcastSubnetAggregate(agg));
                }
            }
            if book.self_id
                == ProposerSchedule::vrf_sortition(&beacon, &set, height, &book.reputation).primary
            {
                let extra = try_emit_mode_a_qc(book, cfg, bp.checkpoint_hash, height, ctx)?;
                for a in extra {
                    actions.push(a);
                }
            }
            Ok(actions)
        }
        AggregationMode::ModeBLeaderless | AggregationMode::Mode0Flat => {
            if mode == AggregationMode::Mode0Flat && bp.subnet.0 != 0 {
                return Ok(Actions::new());
            }
            book.partials
                .entry(bp.checkpoint_hash)
                .or_default()
                .insert(bp.validator);
            let signers_snapshot = book
                .partials
                .get(&bp.checkpoint_hash)
                .cloned()
                .unwrap_or_default();
            if !should_assemble_qc(book, &signers_snapshot) {
                return Ok(Actions::new());
            }
            let candidate = book
                .candidate
                .get(&height)
                .cloned()
                .expect("candidate present for emitted MacroQc");
            let qc = if mode == AggregationMode::ModeBLeaderless {
                macro_qc::try_finalize_mode_b(
                    bp.checkpoint_hash,
                    &signers_snapshot,
                    &book.partial_sigs,
                    &set,
                    &candidate,
                )
            } else {
                macro_qc::try_finalize_mode0(
                    bp.checkpoint_hash,
                    &signers_snapshot,
                    &book.partial_sigs,
                    &set,
                    &candidate,
                )
            };
            let Some(qc) = qc else {
                return Ok(Actions::new());
            };
            let signers_clone = signers_snapshot;
            Ok(finish_macro_qc_adoption(
                book,
                cfg,
                height,
                candidate,
                qc,
                &signers_clone,
            ))
        }
    }
}

/// Idempotent merge of a received MacroQc.
pub fn on_macro_qc_received(
    book: &mut MacroBook,
    cfg: &Config,
    qc: MacroQc,
    ctx: &HostContext<'_>,
) -> Result<Actions> {
    if book.emitted_macro_qc.contains(&qc.checkpoint_hash) {
        return Ok(Actions::new());
    }
    let set = ctx
        .valset
        .set_for(Epoch(0))?
        .ok_or_else(|| crate::Error::InvalidConfig("no validator set for epoch 0".into()))?;
    let signers = book
        .partials
        .get(&qc.checkpoint_hash)
        .cloned()
        .unwrap_or_default();
    let Some(height) = book
        .candidate
        .iter()
        .find(|(_, c)| c.hash == qc.checkpoint_hash)
        .map(|(h, _)| *h)
    else {
        return Ok(Actions::new());
    };
    let candidate = book
        .candidate
        .get(&height)
        .cloned()
        .expect("candidate present at height");
    if !verify::verify_macro_qc(&set, &qc, &candidate) {
        book.rejected_crypto += 1;
        return Ok(Actions::new());
    }
    let mut actions = Actions::new();
    actions.push(Action::PersistMacroCheckpoint(candidate.clone()));
    actions.push(Action::PersistMacroQc(qc.clone()));
    book.emitted_macro_qc.insert(qc.checkpoint_hash);
    book.two_chain.adopt(candidate.clone());
    book.last_macro_hash = candidate.hash;
    book.next_height = Height(height.0 + 1);
    book.proposal_seen.insert(height);
    book.timers.clear_backup(height);
    book.timers.clear_mode_b(height);
    bump_reputation(book, cfg, &signers);
    actions.push(Action::UpdateBlobStatus {
        blob: blob_id_of_checkpoint(&candidate),
        status: crate::api::tier::BlobStatus::Justified,
    });
    let finalized_this_step = book.two_chain.newly_finalized_height().is_some();
    note_inactivity_leak_on_adoption(book, cfg, &mut actions, finalized_this_step);
    if let Some(prev) = book.two_chain.newly_finalized_height() {
        book.two_chain.mark_finalized(prev);
        if let Some(prev_cp) = book.candidate.get(&prev).cloned() {
            actions.push(Action::UpdateBlobStatus {
                blob: blob_id_of_checkpoint(&prev_cp),
                status: crate::api::tier::BlobStatus::Finalized,
            });
        }
    }
    Ok(actions)
}

/// Handle a received subnet aggregate (Mode A).
pub fn on_subnet_aggregate(
    book: &mut MacroBook,
    cfg: &Config,
    agg: SubnetAggregate,
    ctx: &HostContext<'_>,
) -> Result<Actions> {
    if !book.candidate.values().any(|c| c.hash == agg.checkpoint_hash) {
        return Ok(Actions::new());
    }
    let set = ctx
        .valset
        .set_for(Epoch(0))?
        .ok_or_else(|| crate::Error::InvalidConfig("no validator set for epoch 0".into()))?;
    let checkpoint = book
        .candidate
        .values()
        .find(|c| c.hash == agg.checkpoint_hash)
        .cloned()
        .expect("checkpoint for aggregate");
    if !verify::verify_subnet_agg(&set, &agg, &checkpoint, &book.partial_sigs) {
        book.rejected_crypto += 1;
        return Ok(Actions::new());
    }
    let checkpoint_hash = agg.checkpoint_hash;
    book.subnet_aggs
        .entry(checkpoint_hash)
        .or_default()
        .insert(agg.subnet, agg);
    let height = book
        .candidate
        .iter()
        .find(|(_, c)| c.hash == checkpoint_hash)
        .map(|(h, _)| *h)
        .expect("height for aggregate");
    try_emit_mode_a_qc(book, cfg, checkpoint_hash, height, ctx)
}

/// Macro timer fired: backup proposal or Mode B activation.
pub fn on_timer_fired(
    book: &mut MacroBook,
    cfg: &Config,
    ctx: &HostContext<'_>,
    id: TimerId,
    actions: &mut Actions,
) -> Result<()> {
    if let Some(height) = book.timers.height_for_backup_timer(id) {
        book.timers.clear_backup(height);
        if !book.proposal_seen.contains(&height) {
            let Some(candidate) = book.candidate.get(&height).cloned() else {
                return Ok(());
            };
            let set = ctx
                .valset
                .set_for(Epoch(0))?
                .ok_or_else(|| crate::Error::InvalidConfig("no validator set for epoch 0".into()))?;
            let beacon = beacon_for_height(book, ctx, height)?;
            let schedule =
                ProposerSchedule::vrf_sortition(&beacon, &set, height, &book.reputation);
            if schedule.backup == book.self_id {
                let alpha = vrf_alpha(&beacon, height, &book.self_id);
                let (vrf_proof, _) = ctx.signer.vrf_prove(&alpha)?;
                let msg = messages::proposer_message(&book.self_id, &candidate);
                let proposer_sig = ctx.signer.sign_bls(dst::MACRO_PROPOSER_SIG, &msg);
                actions.push(Action::BroadcastMacroProposal(MacroProposal {
                    checkpoint: candidate.clone(),
                    proposer: book.self_id,
                    vrf_proof,
                    proposer_sig,
                }));
                book.proposal_seen.insert(height);
            }
        }
        return Ok(());
    }

    if let Some(height) = book.timers.height_for_mode_b_timer(id) {
        book.timers.clear_mode_b(height);
        let already_finalized = book
            .candidate
            .get(&height)
            .is_some_and(|c| book.emitted_macro_qc.contains(&c.hash));
        if !already_finalized {
            book.mode_b_active.insert(height);
        }
        if book.mode_b_active.contains(&height) {
            if let Some(cp) = book.candidate.get(&height).cloned() {
                if book
                    .locks
                    .try_lock(book.self_id, height, cp.hash)
                    .is_ok()
                {
                    let set = ctx
                        .valset
                        .set_for(Epoch(0))?
                        .ok_or_else(|| {
                            crate::Error::InvalidConfig("no validator set for epoch 0".into())
                        })?;
                    let beacon = beacon_for_height(book, ctx, height)?;
                    let assign = subnet_assign(cfg, &set, &beacon);
                    let subnet = SubnetId(assign.index_for(&book.self_id));
                    book.partials
                        .entry(cp.hash)
                        .or_default()
                        .insert(book.self_id);
                    let checkpoint_msg = messages::checkpoint_message(&cp);
                    let sig = ctx.signer.sign_bls(dst::MACRO_CHECKPOINT, &checkpoint_msg);
                    book.partial_sigs.insert((cp.hash, book.self_id), sig);
                    actions.push(Action::BroadcastBlsPartial(BlsPartial {
                        subnet: if active_mode(book, cfg, &set, height)
                            == AggregationMode::ModeASubnet
                        {
                            subnet
                        } else {
                            SubnetId(0)
                        },
                        validator: book.self_id,
                        checkpoint_hash: cp.hash,
                        sig,
                    }));
                }
            }
        }
    }
    Ok(())
}

/// Deterministic projection from `MacroCheckpoint.hash` to `BlobId` (03c-1 placeholder
/// until L1 lands per-blob granularity).
fn blob_id_of_checkpoint(cp: &MacroCheckpoint) -> BlobId {
    let mut b = [0u8; 32];
    b[..16].copy_from_slice(&cp.hash.0[..16]);
    BlobId(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{Clock, DagView, Persistence, RandomnessBeacon, SignerPort, ValidatorSetPort};
    use crypto::hash::{blake3_with_dst, dst as crypto_dst};
    use types::{
        crypto_types::{BlsAggSig, BlsSig, VrfPubkey, VrfProof},
        macros::{MacroCheckpoint as Cp, MacroQc},
        micro::MicroQc,
        primitives::{Round, StakeWeight, ValidatorId},
        slashing::SlashEvidence,
        validator::{ValidatorEntry, ValidatorIdentity, ValidatorSet},
    };

    struct TestKeyRing {
        bls: Vec<crypto::bls::SecretKey>,
        vrf: Vec<crypto::vrf::VrfKey>,
    }

    impl TestKeyRing {
        fn new(n: u32) -> Self {
            let seed = [0x33; 32];
            let mut bls = Vec::with_capacity(n as usize);
            let mut vrf = Vec::with_capacity(n as usize);
            for i in 0..n {
                let mut label = [0u8; 36];
                label[..32].copy_from_slice(&seed);
                label[32..].copy_from_slice(&i.to_be_bytes());
                bls.push(
                    crypto::bls::SecretKey::from_ikm(&blake3_with_dst(crypto_dst::VALIDATOR_BLS_PARTIAL, &label).0)
                        .unwrap(),
                );
                vrf.push(crypto::vrf::VrfKey::from_seed(
                    &blake3_with_dst(crypto_dst::MACRO_PROPOSER_SIG, &label).0,
                ));
            }
            Self { bls, vrf }
        }

        fn vset(&self, n: u32) -> ValidatorSet {
            let entries = (0..n)
                .map(|i| {
                    let mut id = [0u8; 32];
                    id[..4].copy_from_slice(&i.to_be_bytes());
                    ValidatorEntry {
                        id: ValidatorId(id),
                        bls_pubkey: self.bls[i as usize].public().to_bytes(),
                        vrf_pubkey: VrfPubkey(self.vrf[i as usize].pubkey()),
                        stake: StakeWeight(1),
                        identity: ValidatorIdentity {
                            asn: None,
                            cloud: None,
                            region: None,
                        },
                    }
                })
                .collect();
            ValidatorSet {
                epoch: Epoch(0),
                entries,
                total_stake: StakeWeight(u64::from(n)),
            }
        }
    }

    struct TestSigner<'a> {
        ring: &'a TestKeyRing,
        index: usize,
    }

    impl SignerPort for TestSigner<'_> {
        fn sign_bls(&self, dst_tag: &[u8], msg: &[u8]) -> BlsSig {
            crypto::bls::sign::sign(&self.ring.bls[self.index], dst_tag, msg)
        }

        fn vrf_prove(&self, alpha: &[u8]) -> Result<(VrfProof, Hash32)> {
            Ok(crypto::vrf::vrf_prove(&self.ring.vrf[self.index], alpha))
        }
    }

    struct TestHarness {
        keys: TestKeyRing,
        valset: StaticSet,
    }

    impl TestHarness {
        fn new(n: u32) -> Self {
            let keys = TestKeyRing::new(n);
            let set = keys.vset(n);
            Self {
                keys,
                valset: StaticSet(set),
            }
        }

        fn set(&self) -> &ValidatorSet {
            &self.valset.0
        }

        fn signer(&self, voter_idx: usize) -> TestSigner<'_> {
            TestSigner {
                ring: &self.keys,
                index: voter_idx,
            }
        }

        fn ctx<'a>(&'a self, signer: &'a TestSigner<'a>) -> HostContext<'a> {
            static DAG: EmptyDag = EmptyDag;
            static CLOCK: FixedClock = FixedClock;
            static BEACON: ZeroBeacon = ZeroBeacon;
            static PERSIST: NoopP = NoopP;
            static NO_PENDING: crate::ports::NoPendingBlobs = crate::ports::NoPendingBlobs;
            HostContext {
                dag: &DAG,
                clock: &CLOCK,
                valset: &self.valset,
                beacon: &BEACON,
                persistence: &PERSIST,
                signer,
                pending_blobs: &NO_PENDING,
            }
        }

        fn proposal(&self, proposer_idx: usize, cp: &MacroCheckpoint, beacon: &Hash32) -> MacroProposal {
            let proposer = self.set().entries[proposer_idx].id;
            let alpha = vrf_alpha(beacon, cp.height, &proposer);
            let signer = TestSigner {
                ring: &self.keys,
                index: proposer_idx,
            };
            let (vrf_proof, _) = signer.vrf_prove(&alpha).unwrap();
            let msg = messages::proposer_message(&proposer, cp);
            MacroProposal {
                checkpoint: cp.clone(),
                proposer,
                vrf_proof,
                proposer_sig: signer.sign_bls(crypto_dst::MACRO_PROPOSER_SIG, &msg),
            }
        }

        fn partial(&self, validator_idx: usize, cp: &MacroCheckpoint) -> BlsSig {
            let msg = messages::checkpoint_message(cp);
            TestSigner {
                ring: &self.keys,
                index: validator_idx,
            }
            .sign_bls(crypto_dst::MACRO_CHECKPOINT, &msg)
        }
    }

    struct EmptyDag;
    impl DagView for EmptyDag {
        fn vertex(&self, _h: &Hash32) -> Result<Option<types::dag::CertifiedVertex>> {
            Ok(None)
        }
        fn vertices_at_round(&self, _r: Round) -> Result<Vec<types::dag::CertifiedVertex>> {
            Ok(vec![])
        }
    }
    struct FixedClock;
    impl Clock for FixedClock {
        fn now_nanos(&self) -> u128 {
            0
        }
    }
    struct StaticSet(ValidatorSet);
    impl ValidatorSetPort for StaticSet {
        fn set_for(&self, _e: Epoch) -> Result<Option<ValidatorSet>> {
            Ok(Some(self.0.clone()))
        }
        fn index_of(&self, _e: Epoch, v: &ValidatorId) -> Result<Option<u32>> {
            Ok(self
                .0
                .entries
                .iter()
                .position(|x| &x.id == v)
                .map(|i| u32::try_from(i).unwrap()))
        }
    }
    struct ZeroBeacon;
    impl RandomnessBeacon for ZeroBeacon {
        fn current(&self) -> Result<Hash32> {
            Ok(Hash32::zero())
        }
    }
    struct NoopP;
    impl Persistence for NoopP {
        fn store_micro_qc(&self, _qc: &MicroQc) -> Result<()> {
            Ok(())
        }
        fn micro_qc_for(&self, _h: &Hash32) -> Result<Option<MicroQc>> {
            Ok(None)
        }
        fn store_macro_checkpoint(&self, _cp: &Cp) -> Result<()> {
            Ok(())
        }
        fn store_macro_qc(&self, _qc: &MacroQc) -> Result<()> {
            Ok(())
        }
        fn append_slash_evidence(&self, _e: &SlashEvidence) -> Result<()> {
            Ok(())
        }
        fn macro_checkpoint_at(&self, _h: Height) -> Result<Option<Cp>> {
            Ok(None)
        }
        fn macro_qc_for(&self, _h: &Hash32) -> Result<Option<MacroQc>> {
            Ok(None)
        }
    }

    fn fake_micro_qc(byte: u8) -> MicroQc {
        MicroQc {
            checkpoint_hash: Hash32([byte; 32]),
            agg: BlsAggSig {
                sig: BlsSig([0xAB; 96]),
                bitmap: vec![0xFF],
            },
        }
    }

    fn host_ctx(n: u32) -> TestHarness {
        TestHarness::new(n)
    }

    #[test]
    fn ring_fills_then_primary_emits_proposal() {
        let harness = host_ctx(4);
        let set = harness.set().clone();
        let beacon = Hash32::zero();
        let primary_id =
            ProposerSchedule::vrf_sortition(&beacon, &set, Height(0), &std::collections::HashMap::new())
                .primary;
        let primary_idx = set
            .entries
            .iter()
            .position(|e| e.id == primary_id)
            .unwrap();
        let mut book = MacroBook::new(primary_id);
        let cfg = Config::default_table_17_1();
        let signer = harness.signer(primary_idx);
        let ctx = harness.ctx(&signer);

        for i in 0..cfg.macro_fin.macro_window_w {
            let mut actions = Actions::new();
            actions.push(Action::BroadcastMicroQc(fake_micro_qc(
                u8::try_from(i).expect("macro_window_w fits u8") + 1,
            )));
            on_local_micro_qcs(&mut book, &cfg, &ctx, &mut actions).unwrap();
            if (i as usize) + 1 < cfg.macro_fin.macro_window_w as usize {
                assert_eq!(actions.len(), 1, "no proposal yet at wave {i}");
            } else {
                assert!(
                    actions
                        .iter()
                        .any(|a| matches!(a, Action::BroadcastMacroProposal(_))),
                    "primary emits proposal at wave {i}"
                );
            }
        }
        assert_eq!(book.micro_ring.len(), 0);
        assert_eq!(book.candidate.len(), 1);
    }

    #[test]
    fn non_primary_validator_does_not_emit_proposal() {
        let harness = host_ctx(4);
        let not_primary = harness.set().entries[2].id;
        let mut book = MacroBook::new(not_primary);
        let cfg = Config::default_table_17_1();
        let signer = harness.signer(2);
        let ctx = harness.ctx(&signer);

        for i in 0..cfg.macro_fin.macro_window_w {
            let mut actions = Actions::new();
            actions.push(Action::BroadcastMicroQc(fake_micro_qc(
                u8::try_from(i).expect("macro_window_w fits u8") + 1,
            )));
            on_local_micro_qcs(&mut book, &cfg, &ctx, &mut actions).unwrap();
            assert!(
                !actions
                    .iter()
                    .any(|a| matches!(a, Action::BroadcastMacroProposal(_))),
                "non-primary never emits a proposal at wave {i}"
            );
        }
        assert_eq!(book.candidate.len(), 1, "candidate still built locally");
    }

    #[test]
    fn proposal_from_correct_primary_emits_partial_and_soft_confirmed() {
        let harness = host_ctx(4);
        let set = harness.set().clone();
        let voter = set.entries[2].id;
        let mut book = MacroBook::new(voter);
        let cfg = Config::default_table_17_1();
        let signer = harness.signer(2);
        let ctx = harness.ctx(&signer);
        let beacon = Hash32::zero();

        for i in 0..cfg.macro_fin.macro_window_w {
            let mut actions = Actions::new();
            actions.push(Action::BroadcastMicroQc(fake_micro_qc(
                u8::try_from(i).expect("macro_window_w fits u8") + 1,
            )));
            on_local_micro_qcs(&mut book, &cfg, &ctx, &mut actions).unwrap();
        }
        let candidate = book.candidate.get(&Height(0)).cloned().unwrap();
        let proposer =
            ProposerSchedule::vrf_sortition(&beacon, &set, Height(0), &book.reputation).primary;
        let proposer_idx = set.entries.iter().position(|e| e.id == proposer).unwrap();
        let proposal = harness.proposal(proposer_idx, &candidate, &beacon);
        let actions = on_macro_proposal(&mut book, &cfg, proposal, &ctx).unwrap();
        assert!(actions.iter().any(|a| matches!(a, Action::BroadcastBlsPartial(_))));
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::UpdateBlobStatus { .. }
        )));
    }

    #[test]
    fn proposal_from_wrong_proposer_is_dropped() {
        let harness = host_ctx(4);
        let set = harness.set().clone();
        let voter = set.entries[1].id;
        let mut book = MacroBook::new(voter);
        let cfg = Config::default_table_17_1();
        let signer = harness.signer(1);
        let ctx = harness.ctx(&signer);

        for i in 0..cfg.macro_fin.macro_window_w {
            let mut actions = Actions::new();
            actions.push(Action::BroadcastMicroQc(fake_micro_qc(
                u8::try_from(i).expect("macro_window_w fits u8") + 1,
            )));
            on_local_micro_qcs(&mut book, &cfg, &ctx, &mut actions).unwrap();
        }
        let candidate = book.candidate.get(&Height(0)).cloned().unwrap();
        let beacon = Hash32::zero();
        let schedule =
            ProposerSchedule::vrf_sortition(&beacon, &set, Height(0), &book.reputation);
        let wrong = set
            .entries
            .iter()
            .position(|e| e.id != schedule.primary && e.id != schedule.backup)
            .expect("non-proposer exists for n=4");
        let proposal = harness.proposal(wrong, &candidate, &beacon);
        let actions = on_macro_proposal(&mut book, &cfg, proposal, &ctx).unwrap();
        assert!(actions.is_empty());
    }

    #[test]
    fn lock_macro_collision_blocks_a_second_vote_at_same_height() {
        let v = ValidatorId([0xAA; 32]);
        let mut lm = crate::lock_macro::LockMacro::new();
        lm.try_lock(v, Height(0), Hash32([1; 32])).unwrap();
        let err = lm
            .try_lock(v, Height(0), Hash32([2; 32]))
            .expect_err("conflicting hash must be rejected");
        assert!(err.contains("conflicting"));
    }

    #[test]
    fn bls_partial_threshold_emits_macro_qc_and_justified() {
        let harness = host_ctx(4);
        let set = harness.set().clone();
        let voter = set.entries[0].id;
        let mut book = MacroBook::new(voter);
        let cfg = Config::default_table_17_1();
        let signer = harness.signer(0);
        let ctx = harness.ctx(&signer);

        for i in 0..cfg.macro_fin.macro_window_w {
            let mut actions = Actions::new();
            actions.push(Action::BroadcastMicroQc(fake_micro_qc(
                u8::try_from(i).expect("macro_window_w fits u8") + 1,
            )));
            on_local_micro_qcs(&mut book, &cfg, &ctx, &mut actions).unwrap();
        }
        let candidate = book.candidate.get(&Height(0)).cloned().unwrap();
        let beacon = Hash32::zero();
        let proposer =
            ProposerSchedule::vrf_sortition(&beacon, &set, Height(0), &book.reputation).primary;
        let proposer_idx = set.entries.iter().position(|e| e.id == proposer).unwrap();
        let proposal = harness.proposal(proposer_idx, &candidate, &beacon);
        let _ = on_macro_proposal(&mut book, &cfg, proposal, &ctx).unwrap();
        let mut threshold_actions = None;
        for idx in [0usize, 2, 3] {
            let v = set.entries[idx].id;
            let bp = BlsPartial {
                subnet: SubnetId(0),
                validator: v,
                checkpoint_hash: candidate.hash,
                sig: harness.partial(idx, &candidate),
            };
            let actions = on_bls_partial(&mut book, &cfg, bp, &ctx).unwrap();
            if actions.iter().any(|a| matches!(a, Action::BroadcastMacroQc(_))) {
                threshold_actions = Some(actions);
                break;
            }
        }
        let actions = threshold_actions.expect("threshold met at 2f+1 signers");
        let has_qc = actions.iter().any(|a| matches!(a, Action::BroadcastMacroQc(_)));
        let has_persist_cp = actions
            .iter()
            .any(|a| matches!(a, Action::PersistMacroCheckpoint(_)));
        let has_persist_qc = actions.iter().any(|a| matches!(a, Action::PersistMacroQc(_)));
        let has_justified = actions.iter().any(|a| matches!(
            a,
            Action::UpdateBlobStatus {
                status: crate::api::tier::BlobStatus::Justified,
                ..
            }
        ));
        assert!(has_qc && has_persist_cp && has_persist_qc && has_justified);
        assert_eq!(book.next_height, Height(1));
        assert_eq!(book.last_macro_hash, candidate.hash);
        assert_eq!(book.emitted_macro_qc_count(), 1);
    }

    #[test]
    fn bls_partial_below_threshold_is_silent() {
        let harness = host_ctx(4);
        let set = harness.set().clone();
        let voter = set.entries[1].id;
        let mut book = MacroBook::new(voter);
        let cfg = Config::default_table_17_1();
        let signer = harness.signer(1);
        let ctx = harness.ctx(&signer);

        for i in 0..cfg.macro_fin.macro_window_w {
            let mut actions = Actions::new();
            actions.push(Action::BroadcastMicroQc(fake_micro_qc(
                u8::try_from(i).expect("macro_window_w fits u8") + 1,
            )));
            on_local_micro_qcs(&mut book, &cfg, &ctx, &mut actions).unwrap();
        }
        let candidate = book.candidate.get(&Height(0)).cloned().unwrap();
        let bp = BlsPartial {
            subnet: SubnetId(0),
            validator: set.entries[0].id,
            checkpoint_hash: candidate.hash,
            sig: harness.partial(0, &candidate),
        };
        let actions = on_bls_partial(&mut book, &cfg, bp, &ctx).unwrap();
        assert!(actions.is_empty());
    }

    #[test]
    fn bls_partial_unknown_subnet_or_hash_ignored() {
        let harness = host_ctx(4);
        let set = harness.set().clone();
        let voter = set.entries[1].id;
        let mut book = MacroBook::new(voter);
        let cfg = Config::default_table_17_1();
        let signer = harness.signer(1);
        let ctx = harness.ctx(&signer);

        let bp = BlsPartial {
            subnet: SubnetId(7),
            validator: set.entries[0].id,
            checkpoint_hash: Hash32([1; 32]),
            sig: BlsSig([0; 96]),
        };
        let actions = on_bls_partial(&mut book, &cfg, bp, &ctx).unwrap();
        assert!(actions.is_empty());

        let bp = BlsPartial {
            subnet: SubnetId(0),
            validator: set.entries[0].id,
            checkpoint_hash: Hash32([0xFF; 32]),
            sig: BlsSig([0; 96]),
        };
        let actions = on_bls_partial(&mut book, &cfg, bp, &ctx).unwrap();
        assert!(actions.is_empty());
    }

    #[test]
    fn macro_qc_received_idempotent_after_local_emit() {
        let harness = host_ctx(4);
        let set = harness.set().clone();
        let voter = set.entries[0].id;
        let mut book = MacroBook::new(voter);
        let cfg = Config::default_table_17_1();
        let signer = harness.signer(0);
        let ctx = harness.ctx(&signer);

        for i in 0..cfg.macro_fin.macro_window_w {
            let mut actions = Actions::new();
            actions.push(Action::BroadcastMicroQc(fake_micro_qc(
                u8::try_from(i).expect("macro_window_w fits u8") + 1,
            )));
            on_local_micro_qcs(&mut book, &cfg, &ctx, &mut actions).unwrap();
        }
        let candidate = book.candidate.get(&Height(0)).cloned().unwrap();
        let beacon = Hash32::zero();
        let proposer =
            ProposerSchedule::vrf_sortition(&beacon, &set, Height(0), &book.reputation).primary;
        let proposer_idx = set.entries.iter().position(|e| e.id == proposer).unwrap();
        let proposal = harness.proposal(proposer_idx, &candidate, &beacon);
        let _ = on_macro_proposal(&mut book, &cfg, proposal, &ctx).unwrap();
        for idx in [0usize, 2, 3] {
            let v = set.entries[idx].id;
            let bp = BlsPartial {
                subnet: SubnetId(0),
                validator: v,
                checkpoint_hash: candidate.hash,
                sig: harness.partial(idx, &candidate),
            };
            let _ = on_bls_partial(&mut book, &cfg, bp, &ctx).unwrap();
        }
        let signers = book.partials.get(&candidate.hash).cloned().unwrap();
        let qc = macro_qc::try_finalize_mode0(
            candidate.hash,
            &signers,
            &book.partial_sigs,
            &set,
            &candidate,
        )
        .expect("qc from partials");
        let actions = on_macro_qc_received(&mut book, &cfg, qc, &ctx).unwrap();
        assert!(actions.is_empty());
    }

    #[test]
    fn macro_qc_received_for_unknown_height_dropped() {
        let voter = ValidatorId([9; 32]);
        let mut book = MacroBook::new(voter);
        let cfg = Config::default_table_17_1();
        let harness = host_ctx(4);
        let signer = harness.signer(0);
        let ctx = harness.ctx(&signer);

        let qc = MacroQc {
            checkpoint_hash: Hash32([0x42; 32]),
            mode: types::macros::AggregationMode::Mode0Flat,
            agg: BlsAggSig {
                sig: BlsSig([0xCD; 96]),
                bitmap: vec![],
            },
        };
        let actions = on_macro_qc_received(&mut book, &cfg, qc, &ctx).unwrap();
        assert!(actions.is_empty());
    }

    #[test]
    fn inactivity_leak_emits_after_four_unfinalized_windows() {
        let cfg = Config::default_table_17_1();
        let mut book = MacroBook::new(ValidatorId([0; 32]));
        let mut actions = Actions::new();

        for i in 0..4u64 {
            let height = Height(i);
            let parent = if i == 0 {
                Hash32::zero()
            } else {
                Hash32([0x99; 32])
            };
            let cp = checkpoint::build(height, Epoch(0), parent, Hash32([i as u8 + 1; 32]));
            book.candidate.insert(height, cp.clone());
            book.two_chain.adopt(cp);
            let finalized = book.two_chain.newly_finalized_height().is_some();
            note_inactivity_leak_on_adoption(&mut book, &cfg, &mut actions, finalized);
            if let Some(prev) = book.two_chain.newly_finalized_height() {
                book.two_chain.mark_finalized(prev);
            }
        }

        let leaks: Vec<_> = actions
            .iter()
            .filter(|a| matches!(a, Action::NotifyInactivityLeak { .. }))
            .collect();
        assert_eq!(leaks.len(), 1);
        assert!(matches!(
            leaks[0],
            Action::NotifyInactivityLeak {
                windows: 4,
                bps_per_window: 50,
            }
        ));
    }

    #[test]
    fn inactivity_leak_resets_on_finalization() {
        let cfg = Config::default_table_17_1();
        let mut book = MacroBook::new(ValidatorId([0; 32]));
        let mut actions = Actions::new();
        book.unfinalized_windows = 4;
        book.leak_notified = true;

        note_inactivity_leak_on_adoption(&mut book, &cfg, &mut actions, true);

        assert_eq!(book.unfinalized_windows, 0);
        assert!(!book.leak_notified);
    }
}
