//! The pure deterministic state machine.

use smallvec::SmallVec;

use crate::{action::Action, config::Config, error::Result, event::Event};

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
}

impl StateMachine {
    /// Build a new state machine with the supplied configuration.
    #[must_use]
    pub fn new(cfg: Config) -> Self {
        Self { cfg }
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
    pub fn step(&mut self, event: Event) -> Result<Actions> {
        match event {
            Event::CertifiedVertexReceived(_) => {
                // TODO(plan 03b): Bullshark wave / commit dispatch.
                Ok(Actions::new())
            }
            Event::MicroQcAssembled(_) => Ok(Actions::new()),
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
        let actions = sm.step(Event::TimerFired(TimerId(0))).unwrap();
        assert!(actions.is_empty());
    }

    #[test]
    fn step_is_total_over_event_enum() {
        let mut sm = StateMachine::new(Config::default_table_17_1());
        sm.step(Event::TimerFired(TimerId(0))).unwrap();
    }
}
