//! The pure deterministic state machine.

use smallvec::SmallVec;

use crate::{
    action::Action, config::Config, error::Result, event::Event, host_context::HostContext,
    l2_minimal::Book,
};

/// Up-to-eight outgoing actions per event keeps things stack-allocated.
pub type Actions = SmallVec<[Action; 8]>;

/// Consensus state machine.
///
/// Deterministic: given the same starting state, the same sequence of
/// `Event`s always produces the same sequence of `Action`s.
#[derive(Debug)]
pub struct StateMachine {
    /// Active protocol parameters.
    cfg: Config,
    /// 03b-1 relaxed L2 book (removed in 03b-2).
    book: Book,
}

impl StateMachine {
    /// Build a new state machine with the supplied configuration.
    #[must_use]
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg,
            book: Book::default(),
        }
    }

    /// Active config (immutable while running).
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.cfg
    }

    /// Drive one event through the state machine, returning any
    /// resulting [`Action`]s.
    ///
    /// In the skeleton phase this returns an empty `Actions` for every
    /// event so downstream binaries can wire end-to-end before any
    /// algorithm is implemented.
    pub fn step(&mut self, event: Event, ctx: &HostContext<'_>) -> Result<Actions> {
        match event {
            Event::CertifiedVertexReceived(v) => {
                crate::l2_minimal::on_certified_vertex(&mut self.book, v, ctx)
            }
            Event::MicroQcAssembled(m) => crate::l2_minimal::on_micro_qc_assembled(&mut self.book, m),
            Event::MacroProposalReceived(_) => {
                // TODO(plan 03c): macro proposer dispatch.
                Ok(Actions::new())
            }
            Event::BlsPartialReceived(_) | Event::SubnetAggregateReceived(_) => {
                // TODO(plan 03c): adaptive aggregation.
                Ok(Actions::new())
            }
            Event::MacroQcReceived(_) => Ok(Actions::new()),
            Event::TimerFired(_) => Ok(Actions::new()),
            Event::ValidatorSetUpdated { .. } => Ok(Actions::new()),
            Event::SlashEvidenceFound(_) => {
                // TODO(plan 03d): slashing evidence validation + EmitSlashEvidence.
                Ok(Actions::new())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::TimerId;

    #[test]
    fn step_returns_empty_for_timer_in_skeleton() {
        let mut sm = StateMachine::new(Config::default_table_17_1());
        let ctx = test_host_context();
        let actions = sm
            .step(Event::TimerFired(TimerId(0)), &ctx)
            .unwrap();
        assert!(actions.is_empty());
    }

    #[test]
    fn step_is_total_over_event_enum() {
        let mut sm = StateMachine::new(Config::default_table_17_1());
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

        fn store_macro_checkpoint(
            &self,
            _cp: &types::macros::MacroCheckpoint,
        ) -> Result<()> {
            Ok(())
        }

        fn store_macro_qc(&self, _qc: &types::macros::MacroQc) -> Result<()> {
            Ok(())
        }

        fn append_slash_evidence(
            &self,
            _ev: &types::slashing::SlashEvidence,
        ) -> Result<()> {
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
        HostContext {
            dag: &DAG,
            clock: &CLOCK,
            valset: &VALSET,
            beacon: &BEACON,
            persistence: &PERSIST,
        }
    }
}
