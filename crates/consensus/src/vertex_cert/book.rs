//! Per-validator distributed vertex-certification state (06-04 design §2).

use std::collections::{BTreeMap, HashMap, HashSet};

use types::{
    crypto_types::{BlsSig, Hash32},
    dag::VertexProposal,
    primitives::{Round, ValidatorId},
};

use crate::{event::TimerId, leader::timeout::TimerScheduler};

/// Per-validator vertex-certification state held by `StateMachine`.
#[derive(Debug)]
pub struct VertexBook {
    /// Validator this book belongs to (`author = self` on own proposals).
    pub(crate) self_id: ValidatorId,
    /// Round of this node's latest own proposal.
    pub(crate) current_round: Round,
    /// `genesis_propose` already ran (idempotence guard).
    pub(crate) started: bool,
    /// Certified vertices seen, by round: author → vertex hash.
    /// `BTreeMap` keys give the deterministic author order for parents.
    pub(crate) certified_by_round: BTreeMap<Round, BTreeMap<ValidatorId, Hash32>>,
    /// This node's own proposals being collected, by vertex hash.
    pub(crate) my_proposals: HashMap<Hash32, VertexProposal>,
    /// Partials collected for own proposals: vertex hash → voter → sig.
    pub(crate) collecting: HashMap<Hash32, BTreeMap<ValidatorId, BlsSig>>,
    /// Hashes for which `BroadcastCertifiedVertex` was already emitted.
    pub(crate) emitted_certs: HashSet<Hash32>,
    /// Proposals seen per `(round, author)` → equivocation detection.
    pub(crate) proposals_seen: HashMap<(Round, ValidatorId), Vec<VertexProposal>>,
    /// `(round, author)` pairs this node already voted for.
    pub(crate) voted: HashSet<(Round, ValidatorId)>,
    /// Active fallback timer for `current_round`.
    pub(crate) round_timer: Option<TimerId>,
    /// Consecutive timer fires without round progress (linear backoff).
    pub(crate) timer_retries: u32,
    /// Monotonic timer-id allocator (separate namespace per book).
    pub(crate) timers: TimerScheduler,
    /// Invalid crypto dropped on receive (proposals + partials).
    pub(crate) rejected_crypto: u64,
    /// A voter sent two different sigs for the same vertex (kept first).
    pub(crate) partial_conflicts: u64,
    /// Round-timer fires while the round still lacked `2f+1` certs.
    pub(crate) rounds_stalled: u64,
}

impl VertexBook {
    /// Fresh book for `self_id` at round 0, not yet started.
    #[must_use]
    pub fn new(self_id: ValidatorId) -> Self {
        Self {
            self_id,
            current_round: Round(0),
            started: false,
            certified_by_round: BTreeMap::new(),
            my_proposals: HashMap::new(),
            collecting: HashMap::new(),
            emitted_certs: HashSet::new(),
            proposals_seen: HashMap::new(),
            voted: HashSet::new(),
            round_timer: None,
            timer_retries: 0,
            timers: TimerScheduler::default(),
            rejected_crypto: 0,
            partial_conflicts: 0,
            rounds_stalled: 0,
        }
    }

    /// Round of this node's latest own proposal (sim/test probe).
    #[must_use]
    pub fn current_round(&self) -> u64 {
        self.current_round.0
    }

    /// Test helper: invalid crypto drops.
    #[must_use]
    pub fn rejected_crypto(&self) -> u64 {
        self.rejected_crypto
    }

    /// Test helper: conflicting duplicate partials.
    #[must_use]
    pub fn partial_conflicts(&self) -> u64 {
        self.partial_conflicts
    }

    /// Test helper: stalled-round timer fires.
    #[must_use]
    pub fn rounds_stalled(&self) -> u64 {
        self.rounds_stalled
    }

    /// Number of certified vertices known at `round` (sim/test probe).
    #[must_use]
    pub fn certified_count_at(&self, round: Round) -> usize {
        self.certified_by_round.get(&round).map_or(0, BTreeMap::len)
    }
}
