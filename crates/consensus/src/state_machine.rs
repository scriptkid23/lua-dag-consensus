//! The pure deterministic state machine.

use smallvec::SmallVec;

use crate::{
    action::Action,
    bullshark::{micro_qc::EmittedSet, WaveBook},
    config::Config,
    error::Result,
    event::Event,
    host_context::HostContext,
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
    /// Kept after the `l2_minimal` deletion to keep `MicroQcAssembled` idempotent.
    emitted: EmittedSet,
    /// Committed waves and slow-path timers.
    waves: WaveBook,
}

impl StateMachine {
    /// Build a new state machine with the supplied configuration.
    #[must_use]
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg,
            emitted: EmittedSet::new(),
            waves: WaveBook::new(),
        }
    }

    /// Active config (immutable while running).
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.cfg
    }

    /// Drive one event through the state machine, returning any
    /// resulting [`Action`]s.
    pub fn step(&mut self, event: Event, ctx: &HostContext<'_>) -> Result<Actions> {
        match event {
            Event::CertifiedVertexReceived(cv) => crate::bullshark::on_certified_vertex(
                &mut self.emitted,
                &mut self.waves,
                &self.cfg,
                cv,
                ctx,
            ),
            Event::MicroQcAssembled(qc) => {
                crate::bullshark::on_micro_qc_assembled(&self.emitted, qc)
            }
            Event::TimerFired(id) => crate::bullshark::on_timer_fired(
                &mut self.emitted,
                &mut self.waves,
                &self.cfg,
                id,
                ctx,
            ),
            Event::MacroProposalReceived(_) => {
                // TODO(plan 03c): macro proposer dispatch.
                Ok(Actions::new())
            }
            Event::BlsPartialReceived(_) | Event::SubnetAggregateReceived(_) => {
                // TODO(plan 03c): adaptive aggregation.
                Ok(Actions::new())
            }
            Event::MacroQcReceived(_) => Ok(Actions::new()),
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
    fn step_returns_empty_for_unknown_timer() {
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
