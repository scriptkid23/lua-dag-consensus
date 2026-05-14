//! Macro proposer scheduling (primary + backup).

use types::primitives::{Height, ValidatorId};

/// Primary + backup proposer for a given macro window.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProposerSchedule {
    /// Macro window height.
    pub height: Height,
    /// Primary proposer.
    pub primary: ValidatorId,
    /// Backup proposer (used after `T_macropropose` timeout).
    pub backup: ValidatorId,
}
