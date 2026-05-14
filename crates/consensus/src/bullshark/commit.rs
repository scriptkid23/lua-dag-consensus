//! Bullshark commit rule (shortcut + slow path).

use super::wave::WaveId;

/// Which commit path resolved the wave.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitPath {
    /// Anchor was committed via the 2-round shortcut.
    Shortcut,
    /// Anchor was committed via the 4-round slow path.
    SlowPath,
}

/// Result of running the commit rule for one wave.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitDecision {
    /// Wave that produced the decision.
    pub wave: WaveId,
    /// Which path won.
    pub path: CommitPath,
}
