//! The pure deterministic state machine.

use smallvec::SmallVec;

use types::primitives::ValidatorId;

use crate::{
    action::Action,
    bullshark::{WaveBook, micro_qc::EmittedSet},
    config::Config,
    error::Result,
    event::Event,
    host_context::HostContext,
    macro_fin::MacroBook,
    vertex_cert::VertexBook,
};

/// Up-to-sixteen outgoing actions per event keeps things stack-allocated
/// even when a full Bullshark wave commit fans out across rounds.
pub type Actions = SmallVec<[Action; 16]>;

/// Consensus state machine.
///
/// Deterministic: given the same starting state, the same sequence of
/// `Event`s always produces the same sequence of `Action`s.
#[derive(Debug)]
pub struct StateMachine {
    /// Active protocol parameters.
    cfg: Config,
    /// Checkpoint hashes for which this validator already broadcast a MicroQc.
    emitted: EmittedSet,
    /// Committed waves and slow-path timers.
    waves: WaveBook,
    /// L3 macro-finality bookkeeping (plan 03c-1).
    macros: MacroBook,
    /// L1 distributed vertex certification (06-04 design).
    vertices: VertexBook,
}

impl StateMachine {
    /// Build a new state machine with the supplied configuration and
    /// the validator identity it speaks for (proposer self-check + lock_macro).
    #[must_use]
    pub fn new(cfg: Config, self_id: ValidatorId) -> Self {
        Self {
            cfg,
            emitted: EmittedSet::new(),
            waves: WaveBook::new(),
            macros: MacroBook::new(self_id),
            vertices: VertexBook::new(self_id),
        }
    }

    /// Active config (immutable while running).
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.cfg
    }

    /// Bootstrap the distributed L1 path: propose the round-0 vertex.
    /// Idempotent; hosts call it once before entering the event loop.
    pub fn genesis_propose(&mut self, ctx: &HostContext<'_>) -> Result<Actions> {
        crate::vertex_cert::genesis_propose(&mut self.vertices, &self.cfg, ctx)
    }

    /// Round of this node's latest own vertex proposal (sim/test probe).
    #[must_use]
    pub fn current_vertex_round(&self) -> u64 {
        self.vertices.current_round()
    }

    /// Drive one event through the state machine, returning any
    /// resulting [`Action`]s.
    pub fn step(&mut self, event: Event, ctx: &HostContext<'_>) -> Result<Actions> {
        match event {
            Event::CertifiedVertexReceived(cv) => {
                let mut actions = Actions::new();
                crate::vertex_cert::on_certified_vertex(
                    &mut self.vertices,
                    &self.cfg,
                    &cv,
                    ctx,
                    &mut actions,
                )?;
                let bull = crate::bullshark::on_certified_vertex(
                    &mut self.emitted,
                    &mut self.waves,
                    &self.cfg,
                    cv,
                    ctx,
                )?;
                actions.extend(bull);
                crate::macro_fin::on_local_micro_qcs(
                    &mut self.macros,
                    &self.cfg,
                    ctx,
                    &mut actions,
                )?;
                Ok(actions)
            }
            Event::MicroQcAssembled(qc) => {
                crate::bullshark::on_micro_qc_assembled(&self.emitted, qc)
            }
            Event::TimerFired(id) => {
                let mut actions = crate::bullshark::on_timer_fired(
                    &mut self.emitted,
                    &mut self.waves,
                    &self.cfg,
                    id,
                    ctx,
                )?;
                crate::macro_fin::on_timer_fired(
                    &mut self.macros,
                    &self.cfg,
                    ctx,
                    id,
                    &mut actions,
                )?;
                crate::vertex_cert::on_timer_fired(
                    &mut self.vertices,
                    &self.cfg,
                    ctx,
                    id,
                    &mut actions,
                )?;
                Ok(actions)
            }
            Event::MacroProposalReceived(p) => {
                crate::macro_fin::on_macro_proposal(&mut self.macros, &self.cfg, p, ctx)
            }
            Event::BlsPartialReceived(bp) => {
                crate::macro_fin::on_bls_partial(&mut self.macros, &self.cfg, bp, ctx)
            }
            Event::SubnetAggregateReceived(a) => crate::macro_fin::on_subnet_aggregate(
                &mut self.macros,
                &self.cfg,
                a,
                ctx,
            ),
            Event::MacroQcReceived(qc) => {
                crate::macro_fin::on_macro_qc_received(&mut self.macros, &self.cfg, qc, ctx)
            }
            Event::ValidatorSetUpdated { .. } => Ok(Actions::new()),
            Event::SlashEvidenceFound(_) => Ok(Actions::new()),
            Event::VertexProposalReceived(p) => crate::vertex_cert::on_vertex_proposal(
                &mut self.vertices,
                &self.cfg,
                p,
                ctx,
            ),
            Event::VertexPartialReceived(bp) => crate::vertex_cert::on_vertex_partial(
                &mut self.vertices,
                &self.cfg,
                bp,
                ctx,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::TimerId;

    #[test]
    fn step_returns_empty_for_unknown_timer() {
        let mut sm = StateMachine::new(Config::default_table_17_1(), ValidatorId::default());
        let ctx = test_host_context();
        let actions = sm.step(Event::TimerFired(TimerId(0)), &ctx).unwrap();
        assert!(actions.is_empty());
    }

    #[test]
    fn step_is_total_over_event_enum() {
        let mut sm = StateMachine::new(Config::default_table_17_1(), ValidatorId::default());
        let ctx = test_host_context();
        sm.step(Event::TimerFired(TimerId(0)), &ctx).unwrap();
    }

    struct EmptyDag;
    impl crate::ports::DagView for EmptyDag {
        fn vertex(
            &self,
            _hash: &types::crypto_types::Hash32,
        ) -> Result<Option<types::dag::CertifiedVertex>> {
            Ok(None)
        }

        fn vertices_at_round(
            &self,
            _round: types::primitives::Round,
        ) -> Result<Vec<types::dag::CertifiedVertex>> {
            Ok(vec![])
        }
    }

    struct FixedBeacon(types::crypto_types::Hash32);
    impl crate::ports::RandomnessBeacon for FixedBeacon {
        fn current(&self) -> Result<types::crypto_types::Hash32> {
            Ok(self.0)
        }
    }

    struct EmptyValset;
    impl crate::ports::ValidatorSetPort for EmptyValset {
        fn set_for(
            &self,
            _epoch: types::primitives::Epoch,
        ) -> Result<Option<types::validator::ValidatorSet>> {
            Ok(None)
        }

        fn index_of(
            &self,
            _epoch: types::primitives::Epoch,
            _validator: &types::primitives::ValidatorId,
        ) -> Result<Option<u32>> {
            Ok(None)
        }
    }

    struct NoopPersistence;
    impl crate::ports::Persistence for NoopPersistence {
        fn store_micro_qc(&self, _qc: &types::micro::MicroQc) -> Result<()> {
            Ok(())
        }

        fn micro_qc_for(
            &self,
            _checkpoint_hash: &types::crypto_types::Hash32,
        ) -> Result<Option<types::micro::MicroQc>> {
            Ok(None)
        }

        fn store_macro_checkpoint(&self, _cp: &types::macros::MacroCheckpoint) -> Result<()> {
            Ok(())
        }

        fn store_macro_qc(&self, _qc: &types::macros::MacroQc) -> Result<()> {
            Ok(())
        }

        fn append_slash_evidence(&self, _ev: &types::slashing::SlashEvidence) -> Result<()> {
            Ok(())
        }

        fn macro_checkpoint_at(
            &self,
            _height: types::primitives::Height,
        ) -> Result<Option<types::macros::MacroCheckpoint>> {
            Ok(None)
        }

        fn macro_qc_for(
            &self,
            _checkpoint_hash: &types::crypto_types::Hash32,
        ) -> Result<Option<types::macros::MacroQc>> {
            Ok(None)
        }
    }

    struct TestClock;
    impl crate::ports::Clock for TestClock {
        fn now_nanos(&self) -> u128 {
            0
        }
    }

    fn test_host_context() -> HostContext<'static> {
        static DAG: EmptyDag = EmptyDag;
        static CLOCK: TestClock = TestClock;
        static VALSET: EmptyValset = EmptyValset;
        static BEACON: FixedBeacon = FixedBeacon(types::crypto_types::Hash32::zero());
        static PERSIST: NoopPersistence = NoopPersistence;
        static SIGNER: crate::ports::PanickingSigner = crate::ports::PanickingSigner;
        static NO_PENDING: crate::ports::NoPendingBlobs = crate::ports::NoPendingBlobs;
        HostContext {
            dag: &DAG,
            clock: &CLOCK,
            valset: &VALSET,
            beacon: &BEACON,
            persistence: &PERSIST,
            signer: &SIGNER,
            pending_blobs: &NO_PENDING,
        }
    }
}
