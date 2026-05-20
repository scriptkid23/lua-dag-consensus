//! In-memory L2 book for the 03b-1 relaxed vertical slice.

use std::collections::{HashMap, HashSet};

use types::{crypto_types::Hash32, primitives::{Round, ValidatorId}};

/// Per-wave commit status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaveStatus {
    /// Wave seen but not yet committed.
    Pending,
    /// Wave committed locally.
    Committed,
}

/// Mutable L2 state carried by [`crate::StateMachine`].
#[derive(Debug, Default)]
pub struct Book {
    /// Seen certified vertices (hash → round, author).
    pub seen: HashMap<Hash32, (Round, ValidatorId)>,
    /// Per-wave commit status.
    pub wave_status: HashMap<u64, WaveStatus>,
    /// Checkpoint hashes for which this validator already broadcast a MicroQc.
    pub emitted_micro_qc: HashSet<Hash32>,
}
