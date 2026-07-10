//! `DagView` port: future L1 plug-in seam. Implementations resolve
//! certified vertices by hash and enumerate parents.

use types::{crypto_types::Hash32, dag::SharedCertifiedVertex, primitives::Round};

use crate::error::Result;

/// Read-only view over the availability DAG.
pub trait DagView: Send + Sync {
    /// Return the certified vertex with `hash`, or `None` if unknown.
    fn vertex(&self, hash: &Hash32) -> Result<Option<SharedCertifiedVertex>>;

    /// Return every certified vertex in the given round.
    fn vertices_at_round(&self, round: Round) -> Result<Vec<SharedCertifiedVertex>>;
}
