//! In-memory `DagView` implementation. L1 is placeholder this phase.

use std::{
    collections::HashMap,
    sync::{Mutex, RwLock},
};

use consensus::ports::dag_view::DagView;
use types::{crypto_types::Hash32, dag::CertifiedVertex, primitives::Round};

/// In-memory vertex store.
#[derive(Debug, Default)]
pub struct VirtualDag {
    by_hash: RwLock<HashMap<Hash32, CertifiedVertex>>,
    by_round: Mutex<HashMap<Round, Vec<CertifiedVertex>>>,
}

impl VirtualDag {
    /// Construct empty.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inject a certified vertex (used by scenarios).
    pub fn insert(&self, v: CertifiedVertex) {
        let hash = v.vertex.hash;
        let round = v.vertex.round;
        self.by_round
            .lock()
            .unwrap()
            .entry(round)
            .or_default()
            .push(v.clone());
        self.by_hash.write().unwrap().insert(hash, v);
    }
}

impl DagView for VirtualDag {
    fn vertex(&self, hash: &Hash32) -> consensus::Result<Option<CertifiedVertex>> {
        Ok(self.by_hash.read().unwrap().get(hash).cloned())
    }

    fn vertices_at_round(&self, round: Round) -> consensus::Result<Vec<CertifiedVertex>> {
        Ok(self
            .by_round
            .lock()
            .unwrap()
            .get(&round)
            .cloned()
            .unwrap_or_default())
    }
}
