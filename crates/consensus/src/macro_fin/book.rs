//! In-memory `MacroBook` state and pure helpers for L3 03c-1.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use crypto::hash::{blake3_with_dst, dst, fixture_bls_sig};
use types::{
    crypto_types::{BlsSig, Hash32},
    macros::{MacroCheckpoint, MacroProposal},
    primitives::{Height, ValidatorId},
};

use crate::{
    event::{SubnetAggregate, SubnetId},
    leader::reputation::Reputation,
    lock_macro::LockMacro,
    macro_fin::{timer::MacroTimerBook, two_chain::TwoChainRule, vote_book::VoteBook},
};

/// Per-validator macro-finality state held by `StateMachine`.
#[derive(Debug)]
pub struct MacroBook {
    /// Validator this book belongs to; needed for proposer self-check + lock_macro caller.
    pub(crate) self_id: ValidatorId,
    /// Rolling buffer of the last `W` locally-emitted `MicroQc.checkpoint_hash` values.
    /// Drained when len == W to build the next `MacroCheckpoint` candidate.
    pub(crate) micro_ring: VecDeque<Hash32>,
    /// Next macro height this validator will propose / vote on.
    /// Bootstrap: `Height(0)` covers waves `[0..W)`; genesis parent = `Hash32::zero()`.
    pub(crate) next_height: Height,
    /// Hash of the most recently locally-adopted `MacroCheckpoint` (parent for next candidate).
    pub(crate) last_macro_hash: Hash32,
    /// Pending candidates by height (built locally or received via `MacroProposal`).
    pub(crate) candidate: BTreeMap<Height, MacroCheckpoint>,
    /// `R_macro` frozen when the candidate at `height` was built (subnet + proposer).
    pub(crate) candidate_beacon: BTreeMap<Height, Hash32>,
    /// Partial signers per `checkpoint_hash`.
    pub(crate) partials: HashMap<Hash32, BTreeSet<ValidatorId>>,
    /// Set of `checkpoint_hash`es this validator already emitted a `BroadcastMacroQc` for.
    pub(crate) emitted_macro_qc: HashSet<Hash32>,
    /// 2-chain head + finality state.
    pub(crate) two_chain: TwoChainRule,
    /// Per-validator lock tracker driven on every `BlsPartial` emission.
    pub(crate) locks: LockMacro,
    /// Non-protocol stat: number of times `try_lock` rejected a proposal.
    pub(crate) suppressed_conflicts: u64,
    /// Heights for which a `MacroProposal` was observed locally.
    pub(crate) proposal_seen: HashSet<Height>,
    /// Heights where Mode B leaderless aggregation is active.
    pub(crate) mode_b_active: HashSet<Height>,
    /// Shoal reputation per validator (macro proposer sortition).
    pub(crate) reputation: HashMap<ValidatorId, Reputation>,
    /// Macro-layer timer ids (`T_macropropose`, Mode B deadline).
    pub(crate) timers: MacroTimerBook,
    /// Subnet aggregates received per checkpoint hash.
    pub(crate) subnet_aggs: HashMap<Hash32, HashMap<SubnetId, SubnetAggregate>>,
    /// Partial signers per `(checkpoint_hash, subnet)` for Mode A.
    pub(crate) subnet_partials: HashMap<(Hash32, SubnetId), BTreeSet<ValidatorId>>,
    /// Per-validator vote history (surround detection in 03d).
    pub(crate) votes: VoteBook,
    /// Stored partial BLS sig bytes keyed by `(checkpoint_hash, validator)`.
    pub(crate) partial_sigs: HashMap<(Hash32, ValidatorId), BlsSig>,
    /// Conflicting macro proposals seen per `(height, proposer)`.
    pub(crate) proposals_seen: HashMap<(Height, ValidatorId), Vec<MacroProposal>>,
    /// Invalid crypto events dropped on receive.
    pub(crate) rejected_crypto: u64,
    /// Consecutive macro windows adopted without local finalization.
    pub(crate) unfinalized_windows: u32,
    /// Whether `NotifyInactivityLeak` was already emitted for the current streak.
    pub(crate) leak_notified: bool,
}

impl MacroBook {
    /// Fresh book for `self_id`, bootstrap at `Height(0)` with genesis parent.
    #[must_use]
    pub fn new(self_id: ValidatorId) -> Self {
        Self {
            self_id,
            micro_ring: VecDeque::new(),
            next_height: Height(0),
            last_macro_hash: Hash32::zero(),
            candidate: BTreeMap::new(),
            candidate_beacon: BTreeMap::new(),
            partials: HashMap::new(),
            emitted_macro_qc: HashSet::new(),
            two_chain: TwoChainRule::default(),
            locks: LockMacro::new(),
            suppressed_conflicts: 0,
            proposal_seen: HashSet::new(),
            mode_b_active: HashSet::new(),
            reputation: HashMap::new(),
            timers: MacroTimerBook::new(),
            subnet_aggs: HashMap::new(),
            subnet_partials: HashMap::new(),
            votes: VoteBook::new(),
            partial_sigs: HashMap::new(),
            proposals_seen: HashMap::new(),
            rejected_crypto: 0,
            unfinalized_windows: 0,
            leak_notified: false,
        }
    }

    /// Test helper: count suppressed conflicts.
    #[must_use]
    pub fn suppressed_conflicts(&self) -> u64 {
        self.suppressed_conflicts
    }

    /// Test helper: number of locally-emitted MacroQcs.
    #[must_use]
    pub fn emitted_macro_qc_count(&self) -> usize {
        self.emitted_macro_qc.len()
    }

    /// Test helper: rejected invalid crypto events.
    #[must_use]
    pub fn rejected_crypto(&self) -> u64 {
        self.rejected_crypto
    }
}

/// Deterministic hash of the W most-recent local MicroQc checkpoint hashes.
///
/// Order = local emission order. Under the honest factory both proposer and verifier
/// produce the same root.
#[must_use]
pub fn micro_root_of_ring(ring: &VecDeque<Hash32>) -> Hash32 {
    let mut buf = Vec::with_capacity(32 * ring.len());
    for h in ring {
        buf.extend_from_slice(&h.0);
    }
    blake3_with_dst(dst::MACRO_MICRO_ROOT, &buf)
}

/// Build the partial-signature fixture used by every validator on Mode 0 flat.
#[must_use]
pub fn fixture_partial_sig(validator: &ValidatorId, checkpoint_hash: &Hash32) -> BlsSig {
    BlsSig(fixture_bls_sig(
        dst::VALIDATOR_BLS_PARTIAL,
        &[validator.as_bytes(), &checkpoint_hash.0],
    ))
}

/// Build the proposer-signature fixture used on `MacroProposal`.
#[must_use]
pub fn fixture_proposer_sig(proposer: &ValidatorId, checkpoint_hash: &Hash32) -> BlsSig {
    BlsSig(fixture_bls_sig(
        dst::MACRO_PROPOSER_SIG,
        &[proposer.as_bytes(), &checkpoint_hash.0],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn micro_root_is_deterministic_and_order_sensitive() {
        let mut a = VecDeque::new();
        a.push_back(Hash32([0x11; 32]));
        a.push_back(Hash32([0x22; 32]));
        let mut b = VecDeque::new();
        b.push_back(Hash32([0x22; 32]));
        b.push_back(Hash32([0x11; 32]));
        let ra = micro_root_of_ring(&a);
        let rb = micro_root_of_ring(&b);
        assert_eq!(ra, micro_root_of_ring(&a));
        assert_ne!(ra, rb, "order matters");
    }

    #[test]
    fn fixture_partial_sig_differs_by_validator_and_hash() {
        let v0 = ValidatorId([0; 32]);
        let v1 = ValidatorId([1; 32]);
        let h0 = Hash32([0xAA; 32]);
        let h1 = Hash32([0xBB; 32]);
        let s00 = fixture_partial_sig(&v0, &h0);
        let s10 = fixture_partial_sig(&v1, &h0);
        let s01 = fixture_partial_sig(&v0, &h1);
        assert_ne!(s00, s10);
        assert_ne!(s00, s01);
        assert_eq!(s00, fixture_partial_sig(&v0, &h0));
    }

    #[test]
    fn book_starts_at_genesis() {
        let book = MacroBook::new(ValidatorId([7; 32]));
        assert_eq!(book.next_height, Height(0));
        assert_eq!(book.last_macro_hash, Hash32::zero());
        assert_eq!(book.emitted_macro_qc_count(), 0);
        assert_eq!(book.suppressed_conflicts(), 0);
    }
}
