//! In-memory `DagView` implementation. L1 is placeholder this phase.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, RwLock},
};

use consensus::ports::dag_view::DagView;
use types::{
    crypto_types::Hash32,
    dag::{CertifiedVertex, SharedCertifiedVertex},
    primitives::Round,
};

/// In-memory vertex store.
#[derive(Debug, Default)]
pub struct VirtualDag {
    by_hash: RwLock<HashMap<Hash32, SharedCertifiedVertex>>,
    by_round: Mutex<HashMap<Round, Vec<SharedCertifiedVertex>>>,
}

impl VirtualDag {
    /// Construct empty.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inject a certified vertex (used by scenarios).
    pub fn insert(&self, v: CertifiedVertex) {
        let shared = Arc::new(v);
        let hash = shared.vertex.hash;
        let round = shared.vertex.round;
        self.by_round
            .lock()
            .unwrap()
            .entry(round)
            .or_default()
            .push(Arc::clone(&shared));
        self.by_hash.write().unwrap().insert(hash, shared);
    }
}

impl DagView for VirtualDag {
    fn vertex(&self, hash: &Hash32) -> consensus::Result<Option<SharedCertifiedVertex>> {
        Ok(self.by_hash.read().unwrap().get(hash).cloned())
    }

    fn vertices_at_round(&self, round: Round) -> consensus::Result<Vec<SharedCertifiedVertex>> {
        Ok(self
            .by_round
            .lock()
            .unwrap()
            .get(&round)
            .cloned()
            .unwrap_or_default())
    }
}
