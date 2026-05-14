//! Closure(Aw) BFS linearization, tie-break by vertex hash.

use types::crypto_types::Hash32;

/// Output of linearization: ordered hashes of committed vertices.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Linearization {
    /// Linearized vertex hashes in commit order.
    pub order: Vec<Hash32>,
}
