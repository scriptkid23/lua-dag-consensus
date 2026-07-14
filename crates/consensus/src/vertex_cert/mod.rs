//! L1 distributed vertex certification (06-04 design).
//!
//! Mirrors `macro_fin`: deterministic handlers over a per-validator
//! [`VertexBook`], driven by `StateMachine::step`. The proposer (only)
//! aggregates partials for its own vertex; rounds advance cert-driven
//! (Narwhal) with a re-broadcast fallback timer — never a round jump.

pub mod book;
pub mod verify;

pub use book::VertexBook;

use types::{
    crypto_types::Hash32,
    dag::{CertifiedVertex, Vertex, VertexPartial, VertexProposal},
    primitives::{Epoch, Round},
    validator::ValidatorSet,
};

use crypto::hash::dst;

use crate::{
    action::Action,
    config::Config,
    error::Result,
    event::TimerId,
    host_context::HostContext,
    state_machine::Actions,
};

fn active_set(ctx: &HostContext<'_>) -> Result<ValidatorSet> {
    ctx.valset
        .set_for(Epoch(0))?
        .ok_or_else(|| crate::Error::InvalidConfig("no validator set for epoch 0".into()))
}

fn quorum_need(set: &ValidatorSet) -> Result<usize> {
    let n = u32::try_from(set.entries.len())
        .map_err(|_| crate::Error::InvalidConfig("validator set too large".into()))?;
    Ok(dag::cert::quorum_threshold(n) as usize)
}

fn index_of(set: &ValidatorSet, id: &types::primitives::ValidatorId) -> Option<u32> {
    set.entries
        .iter()
        .position(|e| &e.id == id)
        .map(|i| u32::try_from(i).unwrap_or(u32::MAX))
}

/// Accept messages only for rounds near our own (anti-spam memory bound).
fn round_in_window(book: &VertexBook, round: Round) -> bool {
    round.0 + 1 >= book.current_round.0 && round.0 <= book.current_round.0 + 1
}

fn round_delay_nanos(cfg: &Config, retries: u32) -> u128 {
    u128::from(cfg.timing.round_duration_ms) * 1_000_000 * (u128::from(retries) + 1)
}

/// Handle an inbound vertex proposal: verify, detect equivocation,
/// require certified parents, then vote (one partial per `(round, author)`).
pub fn on_vertex_proposal(
    book: &mut VertexBook,
    _cfg: &Config,
    p: VertexProposal,
    ctx: &HostContext<'_>,
) -> Result<Actions> {
    let mut actions = Actions::new();
    let set = active_set(ctx)?;

    // 1. Integrity + authority. Hash check first — everything signs the body.
    if p.vertex.hash != dag::signing::content_hash(&p.vertex) {
        book.rejected_crypto += 1;
        return Ok(actions);
    }
    if !verify::verify_proposal(&set, &p) {
        book.rejected_crypto += 1;
        return Ok(actions);
    }

    let round = p.vertex.round;
    let author = p.vertex.author;
    if author == book.self_id {
        // Own proposal echoed back via gossip — already seeded locally.
        return Ok(actions);
    }

    // 2. Memory bound: only rounds near our own.
    if !round_in_window(book, round) {
        return Ok(actions);
    }

    // 3. Equivocation: same (round, author), different hash → evidence.
    //    Same hash → known re-broadcast; fall through so a held proposal
    //    can still be voted once its parents certify.
    let key = (round, author);
    match book.proposals_seen.get(&key) {
        Some(existing)
            if existing.iter().any(|prev| prev.vertex.hash != p.vertex.hash) =>
        {
            let first = existing[0].clone();
            actions.push(Action::EmitSlashEvidence {
                offender: author,
                evidence: types::slashing::SlashEvidence::VertexEquivocation(
                    crate::slashing::vertex_equivocation::detect(author, first, p),
                ),
            });
            return Ok(actions);
        }
        Some(_) => {} // identical re-broadcast: already recorded
        None => book.proposals_seen.entry(key).or_default().push(p.clone()),
    }

    // 4. Parents must be ≥2f+1 certified vertices we know at round-1
    //    (genesis round 0 is exempt: parents = []).
    if round.0 > 0 {
        let need = quorum_need(&set)?;
        let prev = book.certified_by_round.get(&Round(round.0 - 1));
        let known = |h: &Hash32| prev.is_some_and(|m| m.values().any(|ph| ph == h));
        if p.vertex.parents.len() < need || !p.vertex.parents.iter().all(known) {
            return Ok(actions); // hold — re-broadcast will retry the vote
        }
    } else if !p.vertex.parents.is_empty() {
        book.rejected_crypto += 1;
        return Ok(actions);
    }

    // 5. One vote per (round, author).
    if book.voted.contains(&key) {
        return Ok(actions);
    }
    book.voted.insert(key);
    let sig = ctx
        .signer
        .sign_bls(dst::VERTEX_CERT, &dag::signing::signing_bytes(&p.vertex));
    actions.push(Action::BroadcastVertexPartial(VertexPartial {
        vertex_hash: p.vertex.hash,
        round,
        author,
        voter: book.self_id,
        sig,
    }));
    Ok(actions)
}

/// Proposer-side vote collection for this node's own proposals.
pub fn on_vertex_partial(
    book: &mut VertexBook,
    cfg: &Config,
    bp: VertexPartial,
    ctx: &HostContext<'_>,
) -> Result<Actions> {
    let mut actions = Actions::new();
    // Only the proposer aggregates its own vertex.
    if bp.author != book.self_id || bp.voter == book.self_id {
        return Ok(actions);
    }
    let Some(proposal) = book.my_proposals.get(&bp.vertex_hash).cloned() else {
        return Ok(actions);
    };
    let slot = book.collecting.entry(bp.vertex_hash).or_default();
    if let Some(prev) = slot.get(&bp.voter) {
        if *prev != bp.sig {
            // Voter equivocation on partials: drop + metric (slashing deferred).
            book.partial_conflicts += 1;
        }
        return Ok(actions);
    }
    let set = active_set(ctx)?;
    if !verify::verify_partial(&set, &bp, &proposal.vertex) {
        book.rejected_crypto += 1;
        return Ok(actions);
    }
    slot.insert(bp.voter, bp.sig);
    try_finalize(book, cfg, ctx, bp.vertex_hash, &mut actions)?;
    Ok(actions)
}

/// Emit `BroadcastCertifiedVertex` once `collecting[hash]` reaches 2f+1.
/// Idempotent per hash. Also self-records the cert and tries to advance.
fn try_finalize(
    book: &mut VertexBook,
    cfg: &Config,
    ctx: &HostContext<'_>,
    hash: Hash32,
    actions: &mut Actions,
) -> Result<()> {
    if book.emitted_certs.contains(&hash) {
        return Ok(());
    }
    let Some(proposal) = book.my_proposals.get(&hash).cloned() else {
        return Ok(());
    };
    let set = active_set(ctx)?;
    let need = quorum_need(&set)?;
    let Some(sigs) = book.collecting.get(&hash) else {
        return Ok(());
    };
    if sigs.len() < need {
        return Ok(());
    }
    let mut contributors = Vec::with_capacity(sigs.len());
    for (voter, sig) in sigs {
        let Some(idx) = index_of(&set, voter) else {
            continue; // verified on receipt; unknown here only on epoch change
        };
        contributors.push((idx, *sig));
    }
    let cv = dag::cert::assemble_cert(&proposal.vertex, &set, &contributors)
        .map_err(|e| crate::Error::InvalidConfig(format!("assemble vertex cert: {e}")))?;
    dag::cert::verify_certified_vertex(&cv, &set)
        .map_err(|e| crate::Error::InvalidConfig(format!("self-check vertex cert: {e}")))?;
    book.emitted_certs.insert(hash);
    record_cert(book, &cv);
    actions.push(Action::BroadcastCertifiedVertex(cv));
    maybe_advance(book, cfg, ctx, actions)?;
    Ok(())
}

/// Record a certified vertex (idempotent by `(round, author)`).
fn record_cert(book: &mut VertexBook, cv: &CertifiedVertex) {
    book.certified_by_round
        .entry(cv.vertex.round)
        .or_default()
        .insert(cv.vertex.author, cv.vertex.hash);
}

/// Build, sign, seed, and broadcast this node's proposal for `round`.
fn propose_round(
    book: &mut VertexBook,
    cfg: &Config,
    ctx: &HostContext<'_>,
    round: Round,
    parents: Vec<Hash32>,
    actions: &mut Actions,
) -> Result<()> {
    let blobs = ctx.pending_blobs.drain();
    let mut vertex = Vertex {
        round,
        author: book.self_id,
        parents,
        blobs: blobs.clone(),
        hash: Hash32::zero(),
    };
    dag::signing::seal_hash(&mut vertex);
    let msg = dag::signing::signing_bytes(&vertex);
    let proposal = VertexProposal {
        vertex: vertex.clone(),
        proposer_sig: ctx.signer.sign_bls(dst::VERTEX_PROPOSAL, &msg),
    };
    // Self-vote: seed our own partial directly — never broadcast it,
    // since only this node aggregates this vertex.
    let self_sig = ctx.signer.sign_bls(dst::VERTEX_CERT, &msg);
    book.my_proposals.insert(vertex.hash, proposal.clone());
    book.collecting
        .entry(vertex.hash)
        .or_default()
        .insert(book.self_id, self_sig);
    book.voted.insert((round, book.self_id));
    book.proposals_seen
        .entry((round, book.self_id))
        .or_default()
        .push(proposal.clone());
    book.current_round = round;

    ctx.pending_blobs.confirm_attached(&blobs);

    if let Some(old) = book.round_timer.take() {
        actions.push(Action::CancelTimer(old));
    }
    let id = book.timers.allocate();
    book.round_timer = Some(id);
    book.timer_retries = 0;
    actions.push(Action::ScheduleTimer {
        id,
        delay_nanos: round_delay_nanos(cfg, 0),
    });
    actions.push(Action::BroadcastVertexProposal(proposal));
    // n = 1 devnet: the self-partial alone is already a quorum.
    try_finalize(book, cfg, ctx, vertex.hash, actions)?;
    Ok(())
}

/// Cert-driven advancement: while round `current_round` holds ≥2f+1
/// certs, propose `current_round + 1` with those certs as parents
/// (first 2f+1 hashes in author order). Idempotent. Prunes old state.
fn maybe_advance(
    book: &mut VertexBook,
    cfg: &Config,
    ctx: &HostContext<'_>,
    actions: &mut Actions,
) -> Result<()> {
    if !book.started {
        return Ok(()); // nothing to advance before genesis_propose
    }
    let set = active_set(ctx)?;
    let need = quorum_need(&set)?;
    loop {
        let r = book.current_round;
        let Some(certs) = book.certified_by_round.get(&r) else {
            break;
        };
        if certs.len() < need {
            break;
        }
        let parents: Vec<Hash32> = certs.values().copied().take(need).collect();
        propose_round(book, cfg, ctx, Round(r.0 + 1), parents, actions)?;
    }
    // Prune everything below current_round - 1 (memory bound §4).
    let keep_from = book.current_round.0.saturating_sub(1);
    book.certified_by_round.retain(|r, _| r.0 >= keep_from);
    book.my_proposals.retain(|_, p| p.vertex.round.0 >= keep_from);
    let live: std::collections::HashSet<Hash32> =
        book.my_proposals.keys().copied().collect();
    book.collecting.retain(|h, _| live.contains(h));
    book.emitted_certs.retain(|h| live.contains(h));
    book.proposals_seen.retain(|(r, _), _| r.0 >= keep_from);
    book.voted.retain(|(r, _)| r.0 >= keep_from);
    Ok(())
}

/// One-shot bootstrap: propose round 0 with no parents.
pub fn genesis_propose(
    book: &mut VertexBook,
    cfg: &Config,
    ctx: &HostContext<'_>,
) -> Result<Actions> {
    let mut actions = Actions::new();
    if book.started {
        return Ok(actions);
    }
    book.started = true;
    propose_round(book, cfg, ctx, Round(0), vec![], &mut actions)?;
    Ok(actions)
}

/// Observe any certified vertex (own loopback or peer) and try to advance.
pub fn on_certified_vertex(
    book: &mut VertexBook,
    cfg: &Config,
    cv: &CertifiedVertex,
    ctx: &HostContext<'_>,
    actions: &mut Actions,
) -> Result<()> {
    record_cert(book, cv);
    maybe_advance(book, cfg, ctx, actions)
}

/// Round fallback: re-broadcast the current proposal, re-arm with linear
/// backoff (capped 8×). Never jumps rounds — parents must stay certified.
pub fn on_timer_fired(
    book: &mut VertexBook,
    cfg: &Config,
    ctx: &HostContext<'_>,
    id: TimerId,
    actions: &mut Actions,
) -> Result<()> {
    if book.round_timer != Some(id) {
        return Ok(());
    }
    book.round_timer = None;
    let set = active_set(ctx)?;
    let need = quorum_need(&set)?;
    if book.certified_count_at(book.current_round) >= need {
        // Quorum already reached; advancement rides the cert events.
        return Ok(());
    }
    book.rounds_stalled += 1;
    if let Some(p) = book
        .my_proposals
        .values()
        .find(|p| p.vertex.round == book.current_round)
    {
        actions.push(Action::BroadcastVertexProposal(p.clone()));
    }
    book.timer_retries = (book.timer_retries + 1).min(8);
    let tid = book.timers.allocate();
    book.round_timer = Some(tid);
    actions.push(Action::ScheduleTimer {
        id: tid,
        delay_nanos: round_delay_nanos(cfg, book.timer_retries),
    });
    Ok(())
}

#[cfg(test)]
pub(crate) mod test_fixture {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use crypto::bls::keys::SecretKey;
    use types::{
        crypto_types::{BlsSig, Hash32, VrfPubkey, VrfProof},
        primitives::{Epoch, StakeWeight, ValidatorId},
        validator::{ValidatorEntry, ValidatorIdentity, ValidatorSet},
    };

    use crate::ports::{
        Clock, DagView, Persistence, RandomnessBeacon, SignerPort,
        ValidatorSetPort,
    };

    pub struct Ring {
        pub sks: Vec<SecretKey>,
        pub set: ValidatorSet,
    }

    impl Ring {
        pub fn new(n: u8) -> Self {
            let sks: Vec<SecretKey> = (0..n)
                .map(|i| SecretKey::from_ikm(&[i + 1; 32]).unwrap())
                .collect();
            let entries = (0..n)
                .map(|i| ValidatorEntry {
                    id: ValidatorId([i; 32]),
                    bls_pubkey: sks[i as usize].public().to_bytes(),
                    vrf_pubkey: VrfPubkey::zero(),
                    stake: StakeWeight(1),
                    identity: ValidatorIdentity {
                        asn: None,
                        cloud: None,
                        region: None,
                    },
                })
                .collect();
            let set = ValidatorSet {
                epoch: Epoch(0),
                entries,
                total_stake: StakeWeight(u64::from(n)),
            };
            Self { sks, set }
        }

        pub fn id(&self, i: u8) -> ValidatorId {
            ValidatorId([i; 32])
        }

        pub fn sign(&self, i: u8, dst: &[u8], msg: &[u8]) -> BlsSig {
            crypto::bls::sign::sign(&self.sks[i as usize], dst, msg)
        }
    }

    pub struct RingSigner<'a> {
        pub ring: &'a Ring,
        pub idx: usize,
    }

    impl SignerPort for RingSigner<'_> {
        fn sign_bls(&self, dst: &[u8], msg: &[u8]) -> BlsSig {
            crypto::bls::sign::sign(&self.ring.sks[self.idx], dst, msg)
        }
        fn vrf_prove(&self, _alpha: &[u8]) -> crate::error::Result<(VrfProof, Hash32)> {
            Ok((VrfProof::zero(), Hash32::zero()))
        }
    }

    pub struct FixedValset(pub ValidatorSet);
    impl ValidatorSetPort for FixedValset {
        fn set_for(&self, epoch: Epoch) -> crate::error::Result<Option<ValidatorSet>> {
            Ok((self.0.epoch == epoch).then(|| self.0.clone()))
        }
        fn index_of(
            &self,
            _epoch: Epoch,
            v: &ValidatorId,
        ) -> crate::error::Result<Option<u32>> {
            Ok(self
                .0
                .entries
                .iter()
                .position(|e| &e.id == v)
                .map(|i| u32::try_from(i).unwrap()))
        }
    }

    pub struct EmptyDag;
    impl DagView for EmptyDag {
        fn vertex(
            &self,
            _h: &Hash32,
        ) -> crate::error::Result<Option<types::dag::SharedCertifiedVertex>> {
            Ok(None)
        }
        fn vertices_at_round(
            &self,
            _r: types::primitives::Round,
        ) -> crate::error::Result<Vec<types::dag::SharedCertifiedVertex>> {
            Ok(vec![])
        }
    }

    pub struct ZeroClock;
    impl Clock for ZeroClock {
        fn now_nanos(&self) -> u128 {
            0
        }
    }

    pub struct ZeroBeacon;
    impl RandomnessBeacon for ZeroBeacon {
        fn current(&self) -> crate::error::Result<Hash32> {
            Ok(Hash32::zero())
        }
    }

    #[derive(Default)]
    pub struct MemPersistence(pub Mutex<HashMap<u8, u8>>);
    impl Persistence for MemPersistence {
        fn store_micro_qc(&self, _qc: &types::micro::MicroQc) -> crate::error::Result<()> {
            Ok(())
        }
        fn micro_qc_for(
            &self,
            _h: &Hash32,
        ) -> crate::error::Result<Option<types::micro::MicroQc>> {
            Ok(None)
        }
        fn store_macro_checkpoint(
            &self,
            _cp: &types::macros::MacroCheckpoint,
        ) -> crate::error::Result<()> {
            Ok(())
        }
        fn store_macro_qc(&self, _qc: &types::macros::MacroQc) -> crate::error::Result<()> {
            Ok(())
        }
        fn append_slash_evidence(
            &self,
            _ev: &types::slashing::SlashEvidence,
        ) -> crate::error::Result<()> {
            Ok(())
        }
        fn macro_checkpoint_at(
            &self,
            _h: types::primitives::Height,
        ) -> crate::error::Result<Option<types::macros::MacroCheckpoint>> {
            Ok(None)
        }
        fn macro_qc_for(
            &self,
            _h: &Hash32,
        ) -> crate::error::Result<Option<types::macros::MacroQc>> {
            Ok(None)
        }
    }

    /// Build a `HostContext` over fixture ports for validator `idx`.
    /// Returns the owned ports; destructure and borrow at the call site:
    ///
    /// ```ignore
    /// let ring = Ring::new(4);
    /// let valset = FixedValset(ring.set.clone());
    /// let signer = RingSigner { ring: &ring, idx: 0 };
    /// let (dag, clock, beacon, persist, no_pending) =
    ///     (EmptyDag, ZeroClock, ZeroBeacon, MemPersistence::default(), NoPendingBlobs);
    /// let ctx = HostContext {
    ///     dag: &dag, clock: &clock, valset: &valset, beacon: &beacon,
    ///     persistence: &persist, signer: &signer, pending_blobs: &no_pending,
    /// };
    /// ```
    pub fn _doc_only() {}
}

#[cfg(test)]
mod proposal_tests {
    use super::test_fixture::*;
    use super::*;
    use crate::ports::NoPendingBlobs;

    fn ctx_parts() -> (EmptyDag, ZeroClock, ZeroBeacon, MemPersistence, NoPendingBlobs) {
        (
            EmptyDag,
            ZeroClock,
            ZeroBeacon,
            MemPersistence::default(),
            NoPendingBlobs,
        )
    }

    fn proposal_from(ring: &Ring, author: u8, round: u64, parents: Vec<Hash32>) -> VertexProposal {
        let mut vertex = Vertex {
            round: Round(round),
            author: ring.id(author),
            parents,
            blobs: vec![],
            hash: Hash32::zero(),
        };
        dag::signing::seal_hash(&mut vertex);
        let sig = ring.sign(
            author,
            dst::VERTEX_PROPOSAL,
            &dag::signing::signing_bytes(&vertex),
        );
        VertexProposal {
            vertex,
            proposer_sig: sig,
        }
    }

    #[test]
    fn valid_genesis_proposal_yields_exactly_one_partial() {
        let ring = Ring::new(4);
        let valset = FixedValset(ring.set.clone());
        let signer = RingSigner { ring: &ring, idx: 1 };
        let (dag_p, clock, beacon, persist, no_pending) = ctx_parts();
        let ctx = HostContext {
            dag: &dag_p,
            clock: &clock,
            valset: &valset,
            beacon: &beacon,
            persistence: &persist,
            signer: &signer,
            pending_blobs: &no_pending,
        };
        let mut book = VertexBook::new(ring.id(1));
        let cfg = Config::default_table_17_1();
        let p = proposal_from(&ring, 0, 0, vec![]);
        let actions = on_vertex_proposal(&mut book, &cfg, p.clone(), &ctx).unwrap();
        assert_eq!(actions.len(), 1);
        let Action::BroadcastVertexPartial(bp) = &actions[0] else {
            panic!("expected partial, got {actions:?}");
        };
        assert_eq!(bp.vertex_hash, p.vertex.hash);
        assert_eq!(bp.author, ring.id(0));
        assert_eq!(bp.voter, ring.id(1));
        assert!(verify::verify_partial(&ring.set, bp, &p.vertex));
        // re-delivery of the same proposal: no second vote
        let again = on_vertex_proposal(&mut book, &cfg, p, &ctx).unwrap();
        assert!(again.is_empty());
    }

    #[test]
    fn bad_proposer_sig_is_dropped() {
        let ring = Ring::new(4);
        let valset = FixedValset(ring.set.clone());
        let signer = RingSigner { ring: &ring, idx: 1 };
        let (dag_p, clock, beacon, persist, no_pending) = ctx_parts();
        let ctx = HostContext {
            dag: &dag_p,
            clock: &clock,
            valset: &valset,
            beacon: &beacon,
            persistence: &persist,
            signer: &signer,
            pending_blobs: &no_pending,
        };
        let mut book = VertexBook::new(ring.id(1));
        let cfg = Config::default_table_17_1();
        let mut p = proposal_from(&ring, 0, 0, vec![]);
        p.proposer_sig = types::crypto_types::BlsSig([0xEE; 96]);
        let actions = on_vertex_proposal(&mut book, &cfg, p, &ctx).unwrap();
        assert!(actions.is_empty());
        assert_eq!(book.rejected_crypto(), 1);
    }

    #[test]
    fn uncertified_parents_hold_then_vote_on_redelivery() {
        let ring = Ring::new(4);
        let valset = FixedValset(ring.set.clone());
        let signer = RingSigner { ring: &ring, idx: 1 };
        let (dag_p, clock, beacon, persist, no_pending) = ctx_parts();
        let ctx = HostContext {
            dag: &dag_p,
            clock: &clock,
            valset: &valset,
            beacon: &beacon,
            persistence: &persist,
            signer: &signer,
            pending_blobs: &no_pending,
        };
        let mut book = VertexBook::new(ring.id(1));
        let cfg = Config::default_table_17_1();
        let parents = vec![Hash32([1; 32]), Hash32([2; 32]), Hash32([3; 32])];
        let p = proposal_from(&ring, 0, 1, parents.clone());
        // round-1 certs unknown → hold, no vote
        let held = on_vertex_proposal(&mut book, &cfg, p.clone(), &ctx).unwrap();
        assert!(held.is_empty());
        // certs for round 0 arrive (3 distinct authors matching the parents)
        for (i, h) in parents.iter().enumerate() {
            book.certified_by_round
                .entry(Round(0))
                .or_default()
                .insert(ring.id(u8::try_from(i).unwrap()), *h);
        }
        // re-delivery (proposer re-broadcast) now votes
        let actions = on_vertex_proposal(&mut book, &cfg, p, &ctx).unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::BroadcastVertexPartial(_)));
    }

    #[test]
    fn double_propose_different_hash_emits_slash_evidence() {
        let ring = Ring::new(4);
        let valset = FixedValset(ring.set.clone());
        let signer = RingSigner { ring: &ring, idx: 1 };
        let (dag_p, clock, beacon, persist, no_pending) = ctx_parts();
        let ctx = HostContext {
            dag: &dag_p,
            clock: &clock,
            valset: &valset,
            beacon: &beacon,
            persistence: &persist,
            signer: &signer,
            pending_blobs: &no_pending,
        };
        let mut book = VertexBook::new(ring.id(1));
        let cfg = Config::default_table_17_1();
        let a = proposal_from(&ring, 0, 0, vec![]);
        let b = proposal_from(&ring, 0, 0, vec![Hash32([9; 32])]); // different content
        assert_ne!(a.vertex.hash, b.vertex.hash);
        let _ = on_vertex_proposal(&mut book, &cfg, a, &ctx).unwrap();
        let actions = on_vertex_proposal(&mut book, &cfg, b, &ctx).unwrap();
        assert_eq!(actions.len(), 1);
        let Action::EmitSlashEvidence { offender, evidence } = &actions[0] else {
            panic!("expected slash evidence, got {actions:?}");
        };
        assert_eq!(*offender, ring.id(0));
        let types::slashing::SlashEvidence::VertexEquivocation(ev) = evidence else {
            panic!("expected VertexEquivocation");
        };
        crate::slashing::vertex_equivocation::verify(ev, &ring.set)
            .expect("emitted evidence must verify offline");
    }

    #[test]
    fn out_of_window_round_is_dropped() {
        let ring = Ring::new(4);
        let valset = FixedValset(ring.set.clone());
        let signer = RingSigner { ring: &ring, idx: 1 };
        let (dag_p, clock, beacon, persist, no_pending) = ctx_parts();
        let ctx = HostContext {
            dag: &dag_p,
            clock: &clock,
            valset: &valset,
            beacon: &beacon,
            persistence: &persist,
            signer: &signer,
            pending_blobs: &no_pending,
        };
        let mut book = VertexBook::new(ring.id(1)); // current_round = 0
        let cfg = Config::default_table_17_1();
        let far_future = proposal_from(&ring, 0, 50, vec![Hash32([1; 32])]);
        let actions = on_vertex_proposal(&mut book, &cfg, far_future, &ctx).unwrap();
        assert!(actions.is_empty());
        assert!(book.proposals_seen.is_empty(), "must not buffer far-future rounds");
    }
}

#[cfg(test)]
mod partial_tests {
    use super::test_fixture::*;
    use super::*;
    use crate::ports::NoPendingBlobs;

    /// Seed `book` with an own proposal at `round` exactly as
    /// `propose_round` (Task 9) will: my_proposals + self-partial + voted.
    fn seed_own_proposal(book: &mut VertexBook, ring: &Ring, idx: u8, round: u64) -> VertexProposal {
        let mut vertex = Vertex {
            round: Round(round),
            author: ring.id(idx),
            parents: vec![],
            blobs: vec![],
            hash: Hash32::zero(),
        };
        dag::signing::seal_hash(&mut vertex);
        let msg = dag::signing::signing_bytes(&vertex);
        let proposal = VertexProposal {
            vertex: vertex.clone(),
            proposer_sig: ring.sign(idx, dst::VERTEX_PROPOSAL, &msg),
        };
        book.my_proposals.insert(vertex.hash, proposal.clone());
        book.collecting
            .entry(vertex.hash)
            .or_default()
            .insert(ring.id(idx), ring.sign(idx, dst::VERTEX_CERT, &msg));
        book.voted.insert((Round(round), ring.id(idx)));
        book.current_round = Round(round);
        proposal
    }

    fn partial(ring: &Ring, voter: u8, p: &VertexProposal) -> VertexPartial {
        VertexPartial {
            vertex_hash: p.vertex.hash,
            round: p.vertex.round,
            author: p.vertex.author,
            voter: ring.id(voter),
            sig: ring.sign(
                voter,
                dst::VERTEX_CERT,
                &dag::signing::signing_bytes(&p.vertex),
            ),
        }
    }

    #[test]
    fn quorum_partials_emit_verifying_certified_vertex() {
        let ring = Ring::new(4);
        let valset = FixedValset(ring.set.clone());
        let signer = RingSigner { ring: &ring, idx: 0 };
        let (dag_p, clock, beacon, persist, no_pending) = (
            EmptyDag,
            ZeroClock,
            ZeroBeacon,
            MemPersistence::default(),
            NoPendingBlobs,
        );
        let ctx = HostContext {
            dag: &dag_p,
            clock: &clock,
            valset: &valset,
            beacon: &beacon,
            persistence: &persist,
            signer: &signer,
            pending_blobs: &no_pending,
        };
        let mut book = VertexBook::new(ring.id(0));
        let cfg = Config::default_table_17_1();
        let p = seed_own_proposal(&mut book, &ring, 0, 0);

        // partial #2 (self + 1 = 2 < 3): below quorum, no cert yet
        let a1 = on_vertex_partial(&mut book, &cfg, partial(&ring, 1, &p), &ctx).unwrap();
        assert!(a1.is_empty());
        // partial #3 reaches 2f+1 = 3 → cert
        let a2 = on_vertex_partial(&mut book, &cfg, partial(&ring, 2, &p), &ctx).unwrap();
        let cv = a2
            .iter()
            .find_map(|a| match a {
                Action::BroadcastCertifiedVertex(cv) => Some(cv.clone()),
                _ => None,
            })
            .expect("cert emitted at quorum");
        dag::cert::verify_certified_vertex(&cv, &ring.set).expect("cert must verify");
        assert_eq!(book.certified_count_at(Round(0)), 1, "self-recorded");
        // late 4th partial: cert already emitted, no duplicate
        let a3 = on_vertex_partial(&mut book, &cfg, partial(&ring, 3, &p), &ctx).unwrap();
        assert!(!a3
            .iter()
            .any(|a| matches!(a, Action::BroadcastCertifiedVertex(_))));
    }

    #[test]
    fn forged_partial_is_dropped() {
        let ring = Ring::new(4);
        let valset = FixedValset(ring.set.clone());
        let signer = RingSigner { ring: &ring, idx: 0 };
        let (dag_p, clock, beacon, persist, no_pending) = (
            EmptyDag,
            ZeroClock,
            ZeroBeacon,
            MemPersistence::default(),
            NoPendingBlobs,
        );
        let ctx = HostContext {
            dag: &dag_p,
            clock: &clock,
            valset: &valset,
            beacon: &beacon,
            persistence: &persist,
            signer: &signer,
            pending_blobs: &no_pending,
        };
        let mut book = VertexBook::new(ring.id(0));
        let cfg = Config::default_table_17_1();
        let p = seed_own_proposal(&mut book, &ring, 0, 0);
        let mut bad = partial(&ring, 1, &p);
        bad.sig = types::crypto_types::BlsSig([0xEE; 96]);
        let actions = on_vertex_partial(&mut book, &cfg, bad, &ctx).unwrap();
        assert!(actions.is_empty());
        assert_eq!(book.rejected_crypto(), 1);
        assert_eq!(book.collecting[&p.vertex.hash].len(), 1, "only self sig");
    }

    #[test]
    fn partial_for_foreign_author_is_ignored() {
        let ring = Ring::new(4);
        let valset = FixedValset(ring.set.clone());
        let signer = RingSigner { ring: &ring, idx: 0 };
        let (dag_p, clock, beacon, persist, no_pending) = (
            EmptyDag,
            ZeroClock,
            ZeroBeacon,
            MemPersistence::default(),
            NoPendingBlobs,
        );
        let ctx = HostContext {
            dag: &dag_p,
            clock: &clock,
            valset: &valset,
            beacon: &beacon,
            persistence: &persist,
            signer: &signer,
            pending_blobs: &no_pending,
        };
        // book belongs to validator 3; partial addressed to author 0
        let mut book = VertexBook::new(ring.id(3));
        let cfg = Config::default_table_17_1();
        let mut other = VertexBook::new(ring.id(0));
        let p = seed_own_proposal(&mut other, &ring, 0, 0);
        let actions = on_vertex_partial(&mut book, &cfg, partial(&ring, 1, &p), &ctx).unwrap();
        assert!(actions.is_empty());
        assert!(book.collecting.is_empty());
    }

    #[test]
    fn duplicate_voter_counts_once_and_conflict_keeps_first() {
        let ring = Ring::new(4);
        let valset = FixedValset(ring.set.clone());
        let signer = RingSigner { ring: &ring, idx: 0 };
        let (dag_p, clock, beacon, persist, no_pending) = (
            EmptyDag,
            ZeroClock,
            ZeroBeacon,
            MemPersistence::default(),
            NoPendingBlobs,
        );
        let ctx = HostContext {
            dag: &dag_p,
            clock: &clock,
            valset: &valset,
            beacon: &beacon,
            persistence: &persist,
            signer: &signer,
            pending_blobs: &no_pending,
        };
        let mut book = VertexBook::new(ring.id(0));
        let cfg = Config::default_table_17_1();
        let p = seed_own_proposal(&mut book, &ring, 0, 0);
        let bp = partial(&ring, 1, &p);
        let _ = on_vertex_partial(&mut book, &cfg, bp.clone(), &ctx).unwrap();
        let _ = on_vertex_partial(&mut book, &cfg, bp.clone(), &ctx).unwrap();
        assert_eq!(book.collecting[&p.vertex.hash].len(), 2); // self + voter 1
        assert_eq!(book.partial_conflicts(), 0);
        // same voter, different (still valid-format) sig bytes → conflict metric
        let mut conflicting = bp;
        conflicting.sig = ring.sign(1, dst::VERTEX_CERT, b"different message");
        let _ = on_vertex_partial(&mut book, &cfg, conflicting, &ctx).unwrap();
        assert_eq!(book.partial_conflicts(), 1);
        assert_eq!(book.collecting[&p.vertex.hash].len(), 2, "first kept");
    }
}

#[cfg(test)]
mod advance_tests {
    use super::test_fixture::*;
    use super::*;
    use crate::ports::NoPendingBlobs;

    struct Fix {
        ring: Ring,
    }

    impl Fix {
        fn new() -> Self {
            Self { ring: Ring::new(4) }
        }
    }

    macro_rules! ctx {
        ($fix:ident, $signer_idx:expr, $valset:ident, $signer:ident, $parts:ident, $host_ctx:ident) => {
            let $valset = FixedValset($fix.ring.set.clone());
            let $signer = RingSigner {
                ring: &$fix.ring,
                idx: $signer_idx,
            };
            let $parts = (
                EmptyDag,
                ZeroClock,
                ZeroBeacon,
                MemPersistence::default(),
                NoPendingBlobs,
            );
            let $host_ctx = HostContext {
                dag: &$parts.0,
                clock: &$parts.1,
                valset: &$valset,
                beacon: &$parts.2,
                persistence: &$parts.3,
                signer: &$signer,
                pending_blobs: &$parts.4,
            };
        };
    }

    fn cert_for(ring: &Ring, author: u8, round: u64) -> CertifiedVertex {
        let mut vertex = Vertex {
            round: Round(round),
            author: ring.id(author),
            parents: vec![],
            blobs: vec![],
            hash: Hash32::zero(),
        };
        dag::signing::seal_hash(&mut vertex);
        let msg = dag::signing::signing_bytes(&vertex);
        let contributors: Vec<_> = (0u8..3)
            .map(|i| (u32::from(i), ring.sign(i, dst::VERTEX_CERT, &msg)))
            .collect();
        dag::cert::assemble_cert(&vertex, &ring.set, &contributors).unwrap()
    }

    #[test]
    fn genesis_propose_emits_proposal_and_timer_and_is_idempotent() {
        let fix = Fix::new();
        ctx!(fix, 0, valset, signer, parts, ctx);
        let mut book = VertexBook::new(fix.ring.id(0));
        let cfg = Config::default_table_17_1();
        let actions = genesis_propose(&mut book, &cfg, &ctx).unwrap();
        let proposal = actions
            .iter()
            .find_map(|a| match a {
                Action::BroadcastVertexProposal(p) => Some(p.clone()),
                _ => None,
            })
            .expect("genesis proposal");
        assert_eq!(proposal.vertex.round, Round(0));
        assert_eq!(proposal.vertex.author, fix.ring.id(0));
        assert!(proposal.vertex.parents.is_empty());
        assert!(verify::verify_proposal(&fix.ring.set, &proposal));
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::ScheduleTimer { .. })),
            "round fallback timer armed"
        );
        // self-partial seeded, not broadcast
        assert_eq!(book.collecting[&proposal.vertex.hash].len(), 1);
        assert!(!actions
            .iter()
            .any(|a| matches!(a, Action::BroadcastVertexPartial(_))));
        // idempotent
        assert!(genesis_propose(&mut book, &cfg, &ctx).unwrap().is_empty());
    }

    #[test]
    fn quorum_certs_advance_round_with_author_ordered_parents() {
        let fix = Fix::new();
        ctx!(fix, 0, valset, signer, parts, ctx);
        let mut book = VertexBook::new(fix.ring.id(0));
        let cfg = Config::default_table_17_1();
        let _ = genesis_propose(&mut book, &cfg, &ctx).unwrap();

        // two peer certs: not enough (own cert not yet formed → 2 < 3)
        let mut actions = Actions::new();
        on_certified_vertex(&mut book, &cfg, &cert_for(&fix.ring, 1, 0), &ctx, &mut actions)
            .unwrap();
        on_certified_vertex(&mut book, &cfg, &cert_for(&fix.ring, 2, 0), &ctx, &mut actions)
            .unwrap();
        assert_eq!(book.current_round(), 0);
        assert!(actions.is_empty());

        // third cert reaches 2f+1 → propose round 1
        on_certified_vertex(&mut book, &cfg, &cert_for(&fix.ring, 3, 0), &ctx, &mut actions)
            .unwrap();
        assert_eq!(book.current_round(), 1);
        let proposal = actions
            .iter()
            .find_map(|a| match a {
                Action::BroadcastVertexProposal(p) => Some(p.clone()),
                _ => None,
            })
            .expect("round-1 proposal");
        assert_eq!(proposal.vertex.round, Round(1));
        assert_eq!(proposal.vertex.parents.len(), 3);
        // parents = cert hashes ordered by author id (BTreeMap order)
        let expected: Vec<Hash32> = book.certified_by_round[&Round(0)]
            .values()
            .copied()
            .take(3)
            .collect();
        assert_eq!(proposal.vertex.parents, expected);
        // duplicate cert does not double-advance
        let mut again = Actions::new();
        on_certified_vertex(&mut book, &cfg, &cert_for(&fix.ring, 3, 0), &ctx, &mut again)
            .unwrap();
        assert_eq!(book.current_round(), 1);
        assert!(again.is_empty());
    }

    #[test]
    fn timer_fire_rebroadcasts_same_proposal_and_never_jumps() {
        let fix = Fix::new();
        ctx!(fix, 0, valset, signer, parts, ctx);
        let mut book = VertexBook::new(fix.ring.id(0));
        let cfg = Config::default_table_17_1();
        let genesis = genesis_propose(&mut book, &cfg, &ctx).unwrap();
        let first = genesis
            .iter()
            .find_map(|a| match a {
                Action::BroadcastVertexProposal(p) => Some(p.clone()),
                _ => None,
            })
            .unwrap();
        let timer_id = book.round_timer.expect("armed");

        let mut actions = Actions::new();
        on_timer_fired(&mut book, &cfg, &ctx, timer_id, &mut actions).unwrap();
        assert_eq!(book.current_round(), 0, "no round jump");
        assert_eq!(book.rounds_stalled(), 1);
        let rebroadcast = actions
            .iter()
            .find_map(|a| match a {
                Action::BroadcastVertexProposal(p) => Some(p.clone()),
                _ => None,
            })
            .expect("re-broadcast");
        assert_eq!(rebroadcast.vertex.hash, first.vertex.hash, "immutable within round");
        // re-armed with linear backoff: 2 × round_duration on first retry
        let Some(Action::ScheduleTimer { delay_nanos, .. }) = actions
            .iter()
            .find(|a| matches!(a, Action::ScheduleTimer { .. }))
        else {
            panic!("timer re-armed");
        };
        assert_eq!(
            *delay_nanos,
            u128::from(cfg.timing.round_duration_ms) * 1_000_000 * 2
        );
        // a stale/foreign timer id is ignored
        let mut noop = Actions::new();
        on_timer_fired(&mut book, &cfg, &ctx, TimerId(9_999), &mut noop).unwrap();
        assert!(noop.is_empty());
    }

    #[test]
    fn old_rounds_are_pruned_after_advance() {
        let fix = Fix::new();
        ctx!(fix, 0, valset, signer, parts, ctx);
        let mut book = VertexBook::new(fix.ring.id(0));
        let cfg = Config::default_table_17_1();
        let _ = genesis_propose(&mut book, &cfg, &ctx).unwrap();
        let mut actions = Actions::new();
        for round in 0u64..3 {
            for author in 1u8..4 {
                on_certified_vertex(
                    &mut book,
                    &cfg,
                    &cert_for(&fix.ring, author, round),
                    &ctx,
                    &mut actions,
                )
                .unwrap();
            }
        }
        assert_eq!(book.current_round(), 3);
        // rounds below current-1 = 2 pruned
        assert!(!book.certified_by_round.contains_key(&Round(0)));
        assert!(!book.certified_by_round.contains_key(&Round(1)));
        assert!(book.certified_by_round.contains_key(&Round(2)));
        assert!(book.my_proposals.values().all(|p| p.vertex.round.0 + 1 >= 3));
    }
}

#[cfg(test)]
mod pending_blob_hook_tests {
    use std::sync::Mutex;

    use super::test_fixture::*;
    use super::*;
    use crate::ports::PendingBlobSource;
    use types::{crypto_types::Hash32, dag::BlobRef, primitives::BlobId};

    struct RecordingPending {
        queue: Mutex<Vec<BlobRef>>,
        confirm_log: Mutex<Vec<Vec<BlobId>>>,
        boot_done: bool,
    }

    impl RecordingPending {
        fn with_blobs(blobs: Vec<BlobRef>, boot_done: bool) -> Self {
            Self {
                queue: Mutex::new(blobs),
                confirm_log: Mutex::new(Vec::new()),
                boot_done,
            }
        }
    }

    impl PendingBlobSource for RecordingPending {
        fn drain(&self) -> Vec<BlobRef> {
            if !self.boot_done {
                return Vec::new();
            }
            self.queue.lock().expect("lock").drain(..).collect()
        }

        fn confirm_attached(&self, blobs: &[BlobRef]) {
            self.confirm_log
                .lock()
                .expect("lock")
                .push(blobs.iter().map(|b| b.blob_id).collect());
        }
    }

    fn sample_blob(byte: u8) -> BlobRef {
        BlobRef {
            blob_id: BlobId([byte; 32]),
            commitment: Hash32([byte; 32]),
            size_bytes: 1024,
        }
    }

    #[test]
    fn propose_blocked_until_boot_sync_done() {
        let ring = Ring::new(4);
        let valset = FixedValset(ring.set.clone());
        let signer = RingSigner { ring: &ring, idx: 0 };
        let pending = RecordingPending::with_blobs(vec![sample_blob(0x01)], false);
        let parts = (
            EmptyDag,
            ZeroClock,
            ZeroBeacon,
            MemPersistence::default(),
        );
        let ctx = HostContext {
            dag: &parts.0,
            clock: &parts.1,
            valset: &valset,
            beacon: &parts.2,
            persistence: &parts.3,
            signer: &signer,
            pending_blobs: &pending,
        };
        let mut book = VertexBook::new(ring.id(0));
        let cfg = Config::default_table_17_1();
        let actions = genesis_propose(&mut book, &cfg, &ctx).unwrap();
        let proposal = actions
            .iter()
            .find_map(|a| match a {
                Action::BroadcastVertexProposal(p) => Some(p),
                _ => None,
            })
            .expect("proposal");
        assert!(proposal.vertex.blobs.is_empty());
    }

    #[test]
    fn confirm_attached_called_after_propose() {
        let ring = Ring::new(4);
        let valset = FixedValset(ring.set.clone());
        let signer = RingSigner { ring: &ring, idx: 0 };
        let blob = sample_blob(0x02);
        let pending = RecordingPending::with_blobs(vec![blob], true);
        let parts = (
            EmptyDag,
            ZeroClock,
            ZeroBeacon,
            MemPersistence::default(),
        );
        let ctx = HostContext {
            dag: &parts.0,
            clock: &parts.1,
            valset: &valset,
            beacon: &parts.2,
            persistence: &parts.3,
            signer: &signer,
            pending_blobs: &pending,
        };
        let mut book = VertexBook::new(ring.id(0));
        let cfg = Config::default_table_17_1();
        let actions = genesis_propose(&mut book, &cfg, &ctx).unwrap();
        let proposal = actions
            .iter()
            .find_map(|a| match a {
                Action::BroadcastVertexProposal(p) => Some(p),
                _ => None,
            })
            .expect("proposal");
        assert_eq!(proposal.vertex.blobs.len(), 1);
        let log = pending.confirm_log.lock().expect("lock");
        assert_eq!(log.len(), 1);
        assert_eq!(log[0], vec![blob.blob_id]);
    }
}
