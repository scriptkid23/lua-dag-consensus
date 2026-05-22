//! L3 macro-finality (whitepaper §9).

pub mod aggregation;
pub mod book;
pub mod checkpoint;
pub mod macro_qc;
pub mod proposer;
pub mod timer;
pub mod two_chain;
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
    primitives::{BlobId, Epoch, Height},
    validator::ValidatorSet,
};

use crate::{
    action::Action,
    config::Config,
    error::Result,
    event::{BlsPartial, SubnetAggregate, SubnetId, TimerId},
    host_context::HostContext,
    leader::reputation::Reputation,
    state_machine::Actions,
};

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
    let Some(qc) = macro_qc::try_finalize_mode_a_from_aggs(checkpoint_hash, &aggs, &set) else {
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
            actions.push(Action::BroadcastMacroProposal(MacroProposal {
                checkpoint: candidate.clone(),
                proposer: book.self_id,
                vrf_proof: ProposerSchedule::vrf_proof_for(&beacon, candidate_height, &book.self_id),
                proposer_sig: book::fixture_proposer_sig(&book.self_id, &candidate.hash),
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
    book.votes.record(
        book.self_id,
        VoteRecord {
            source: Epoch(0),
            target: Epoch(0),
            checkpoint: p.checkpoint.hash,
        },
    );

    let assign = subnet_assign(cfg, &set, &beacon);
    let subnet = SubnetId(assign.index_for(&book.self_id));
    let sig = book::fixture_partial_sig(&book.self_id, &p.checkpoint.hash);
    let mut actions = Actions::new();
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
            let qc = if mode == AggregationMode::ModeBLeaderless {
                macro_qc::try_finalize_mode_b(bp.checkpoint_hash, &signers_snapshot, &set)
            } else {
                macro_qc::try_finalize_mode0(bp.checkpoint_hash, &signers_snapshot, &set)
            };
            let Some(qc) = qc else {
                return Ok(Actions::new());
            };
            let candidate = book
                .candidate
                .get(&height)
                .cloned()
                .expect("candidate present for emitted MacroQc");
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
    _ctx: &HostContext<'_>,
) -> Result<Actions> {
    if book.emitted_macro_qc.contains(&qc.checkpoint_hash) {
        return Ok(Actions::new());
    }
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
                actions.push(Action::BroadcastMacroProposal(MacroProposal {
                    checkpoint: candidate.clone(),
                    proposer: book.self_id,
                    vrf_proof: ProposerSchedule::vrf_proof_for(&beacon, height, &book.self_id),
                    proposer_sig: book::fixture_proposer_sig(&book.self_id, &candidate.hash),
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
                        sig: book::fixture_partial_sig(&book.self_id, &cp.hash),
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
    use crate::ports::{Clock, DagView, Persistence, RandomnessBeacon, ValidatorSetPort};
    use types::{
        crypto_types::{BlsAggSig, BlsPubkey, BlsSig, VrfProof},
        macros::{MacroCheckpoint as Cp, MacroQc},
        micro::MicroQc,
        primitives::{Round, StakeWeight, ValidatorId},
        slashing::SlashEvidence,
        validator::{ValidatorEntry, ValidatorIdentity, ValidatorSet},
    };

    fn vset(n: u32) -> ValidatorSet {
        let entries = (0..n)
            .map(|i| {
                let mut id = [0u8; 32];
                id[..4].copy_from_slice(&i.to_be_bytes());
                ValidatorEntry {
                    id: ValidatorId(id),
                    bls_pubkey: BlsPubkey([0; 48]),
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

    fn host_ctx(set: ValidatorSet) -> TestHost {
        TestHost {
            valset: StaticSet(set),
        }
    }

    struct TestHost {
        valset: StaticSet,
    }

    impl TestHost {
        fn ctx(&self) -> HostContext<'_> {
            static DAG: EmptyDag = EmptyDag;
            static CLOCK: FixedClock = FixedClock;
            static BEACON: ZeroBeacon = ZeroBeacon;
            static PERSIST: NoopP = NoopP;
            HostContext {
                dag: &DAG,
                clock: &CLOCK,
                valset: &self.valset,
                beacon: &BEACON,
                persistence: &PERSIST,
            }
        }
    }

    #[test]
    fn ring_fills_then_primary_emits_proposal() {
        let set = vset(4);
        let beacon = Hash32::zero();
        let primary_id =
            ProposerSchedule::vrf_sortition(&beacon, &set, Height(0), &std::collections::HashMap::new())
                .primary;
        let mut book = MacroBook::new(primary_id);
        let cfg = Config::default_table_17_1();
        let host = host_ctx(set);
        let ctx = host.ctx();

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
        let set = vset(4);
        let not_primary = set.entries[2].id;
        let mut book = MacroBook::new(not_primary);
        let cfg = Config::default_table_17_1();
        let host = host_ctx(set);
        let ctx = host.ctx();

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
        let set = vset(4);
        let voter = set.entries[2].id;
        let mut book = MacroBook::new(voter);
        let cfg = Config::default_table_17_1();
        let host = host_ctx(set.clone());
        let ctx = host.ctx();
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
        let proposal = MacroProposal {
            checkpoint: candidate.clone(),
            proposer,
            vrf_proof: ProposerSchedule::vrf_proof_for(&beacon, Height(0), &proposer),
            proposer_sig: book::fixture_proposer_sig(&proposer, &candidate.hash),
        };
        let actions = on_macro_proposal(&mut book, &cfg, proposal, &ctx).unwrap();
        assert_eq!(actions.len(), 2);
        assert!(matches!(&actions[0], Action::BroadcastBlsPartial(_)));
        assert!(matches!(&actions[1], Action::UpdateBlobStatus { .. }));
    }

    #[test]
    fn proposal_from_wrong_proposer_is_dropped() {
        let set = vset(4);
        let voter = set.entries[1].id;
        let mut book = MacroBook::new(voter);
        let cfg = Config::default_table_17_1();
        let host = host_ctx(set.clone());
        let ctx = host.ctx();

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
            .map(|e| e.id)
            .find(|id| *id != schedule.primary && *id != schedule.backup)
            .expect("non-proposer exists for n=4");
        let proposal = MacroProposal {
            checkpoint: candidate.clone(),
            proposer: wrong,
            vrf_proof: ProposerSchedule::vrf_proof_for(&beacon, Height(0), &wrong),
            proposer_sig: book::fixture_proposer_sig(&wrong, &candidate.hash),
        };
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
        let set = vset(4);
        // Lex-min signer assembles the QC (`should_assemble_qc`).
        let voter = set.entries[0].id;
        let mut book = MacroBook::new(voter);
        let cfg = Config::default_table_17_1();
        let host = host_ctx(set.clone());
        let ctx = host.ctx();

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
        let proposal = MacroProposal {
            checkpoint: candidate.clone(),
            proposer,
            vrf_proof: ProposerSchedule::vrf_proof_for(&beacon, Height(0), &proposer),
            proposer_sig: book::fixture_proposer_sig(&proposer, &candidate.hash),
        };
        let _ = on_macro_proposal(&mut book, &cfg, proposal, &ctx).unwrap();
        let mut threshold_actions = None;
        for v in [set.entries[0].id, set.entries[2].id, set.entries[3].id] {
            let bp = BlsPartial {
                subnet: SubnetId(0),
                validator: v,
                checkpoint_hash: candidate.hash,
                sig: book::fixture_partial_sig(&v, &candidate.hash),
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
        let set = vset(4);
        let voter = set.entries[1].id;
        let mut book = MacroBook::new(voter);
        let cfg = Config::default_table_17_1();
        let host = host_ctx(set.clone());
        let ctx = host.ctx();

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
            sig: book::fixture_partial_sig(&set.entries[0].id, &candidate.hash),
        };
        let actions = on_bls_partial(&mut book, &cfg, bp, &ctx).unwrap();
        assert!(actions.is_empty());
    }

    #[test]
    fn bls_partial_unknown_subnet_or_hash_ignored() {
        let set = vset(4);
        let voter = set.entries[1].id;
        let mut book = MacroBook::new(voter);
        let cfg = Config::default_table_17_1();
        let host = host_ctx(set.clone());
        let ctx = host.ctx();

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
        let set = vset(4);
        let voter = set.entries[0].id;
        let mut book = MacroBook::new(voter);
        let cfg = Config::default_table_17_1();
        let host = host_ctx(set.clone());
        let ctx = host.ctx();

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
        let proposal = MacroProposal {
            checkpoint: candidate.clone(),
            proposer,
            vrf_proof: ProposerSchedule::vrf_proof_for(&beacon, Height(0), &proposer),
            proposer_sig: book::fixture_proposer_sig(&proposer, &candidate.hash),
        };
        let _ = on_macro_proposal(&mut book, &cfg, proposal, &ctx).unwrap();
        for v in [set.entries[0].id, set.entries[2].id, set.entries[3].id] {
            let bp = BlsPartial {
                subnet: SubnetId(0),
                validator: v,
                checkpoint_hash: candidate.hash,
                sig: book::fixture_partial_sig(&v, &candidate.hash),
            };
            let _ = on_bls_partial(&mut book, &cfg, bp, &ctx).unwrap();
        }
        let qc = MacroQc {
            checkpoint_hash: candidate.hash,
            mode: types::macros::AggregationMode::Mode0Flat,
            agg: BlsAggSig {
                sig: BlsSig([0xCD; 96]),
                bitmap: vec![0b1111],
            },
        };
        let actions = on_macro_qc_received(&mut book, &cfg, qc, &ctx).unwrap();
        assert!(actions.is_empty());
    }

    #[test]
    fn macro_qc_received_for_unknown_height_dropped() {
        let voter = ValidatorId([9; 32]);
        let mut book = MacroBook::new(voter);
        let cfg = Config::default_table_17_1();
        let host = host_ctx(vset(4));
        let ctx = host.ctx();

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
}
