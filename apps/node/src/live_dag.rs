//! Live L1 DAG view: in-memory index + RocksDB vertex persistence (plan 06b-L1).

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use consensus::ports::DagView;
use storage::{db::Database, stores::vertex_store};
use types::{
    crypto_types::Hash32,
    dag::{CertifiedVertex, SharedCertifiedVertex},
    primitives::{Round, ValidatorId},
};

/// Thread-safe DAG backed by memory and the `vertex` column family.
#[derive(Debug)]
pub struct LiveDag {
    by_hash: RwLock<HashMap<Hash32, SharedCertifiedVertex>>,
    by_round: RwLock<HashMap<Round, Vec<SharedCertifiedVertex>>>,
    db: Arc<Database>,
}

impl LiveDag {
    /// Wrap an open database handle.
    #[must_use]
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            by_hash: RwLock::new(HashMap::new()),
            by_round: RwLock::new(HashMap::new()),
            db,
        }
    }

    /// Persist and index an inbound certified vertex.
    pub fn ingest(&self, v: CertifiedVertex) -> consensus::Result<()> {
        vertex_store::put(&self.db, &v).map_err(|e| {
            consensus::Error::Persistence(format!("store certified vertex: {e}"))
        })?;
        let shared = Arc::new(v);
        let hash = shared.vertex.hash;
        let round = shared.vertex.round;
        self.by_hash
            .write()
            .expect("dag hash lock")
            .insert(hash, Arc::clone(&shared));
        self.by_round
            .write()
            .expect("dag round lock")
            .entry(round)
            .or_default()
            .push(shared);
        Ok(())
    }

    /// Load a vertex from RocksDB into the memory index on cache miss.
    #[allow(dead_code)]
    fn load_from_store(
        &self,
        round: Round,
        author: &ValidatorId,
    ) -> consensus::Result<Option<CertifiedVertex>> {
        let v = vertex_store::get(&self.db, round, author).map_err(|e| {
            consensus::Error::Persistence(format!("load certified vertex: {e}"))
        })?;
        if let Some(vertex) = v {
            let ret = vertex.clone();
            let shared = Arc::new(vertex);
            let hash = shared.vertex.hash;
            self.by_hash
                .write()
                .expect("dag hash lock")
                .insert(hash, Arc::clone(&shared));
            self.by_round
                .write()
                .expect("dag round lock")
                .entry(round)
                .or_default()
                .push(shared);
            return Ok(Some(ret));
        }
        Ok(None)
    }

    /// Certified vertex hashes in `from..=to` from the live L1 DAG index.
    #[must_use]
    pub fn certified_hashes_in_range(&self, from: Round, to: Round) -> Vec<Hash32> {
        let mut out = Vec::new();
        if from.0 > to.0 {
            return out;
        }
        for round in from.0..=to.0 {
            if let Ok(batch) = self.vertices_at_round(Round(round)) {
                for cv in batch {
                    out.push(cv.vertex.hash);
                }
            }
        }
        out
    }
}

impl DagView for LiveDag {
    fn vertex(&self, hash: &Hash32) -> consensus::Result<Option<SharedCertifiedVertex>> {
        Ok(self
            .by_hash
            .read()
            .expect("dag hash lock")
            .get(hash)
            .cloned())
    }

    fn vertices_at_round(&self, round: Round) -> consensus::Result<Vec<SharedCertifiedVertex>> {
        Ok(self
            .by_round
            .read()
            .expect("dag round lock")
            .get(&round)
            .cloned()
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use storage::{config::StorageConfig, db::Database};
    use types::dag::Vertex;

    fn sample_vertex(round: u64, author_byte: u8) -> CertifiedVertex {
        CertifiedVertex {
            vertex: Vertex {
                round: Round(round),
                author: ValidatorId([author_byte; 32]),
                parents: vec![],
                blobs: vec![],
                hash: Hash32([author_byte; 32]),
            },
            certificate: types::crypto_types::BlsAggSig {
                sig: types::crypto_types::BlsSig([0; 96]),
                bitmap: vec![0xFF],
            },
        }
    }

    #[test]
    fn ingest_round_trips_through_dag_view() {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(
            Database::open(&StorageConfig {
                path: dir.path().to_path_buf(),
                create_if_missing: true,
                max_total_wal_size_mb: 16,
            })
            .unwrap(),
        );
        let dag = LiveDag::new(db);
        let v = sample_vertex(3, 7);
        dag.ingest(v.clone()).unwrap();
        let got = dag.vertex(&v.vertex.hash).unwrap().unwrap();
        assert_eq!(*got, v);
        assert_eq!(dag.vertices_at_round(Round(3)).unwrap().len(), 1);
    }

    #[test]
    fn ingest_shares_one_allocation_across_indexes() {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(
            Database::open(&StorageConfig {
                path: dir.path().to_path_buf(),
                create_if_missing: true,
                max_total_wal_size_mb: 16,
            })
            .unwrap(),
        );
        let dag = LiveDag::new(db);
        let v = sample_vertex(1, 2);
        dag.ingest(v).unwrap();
        let by_hash = dag.vertex(&Hash32([2; 32])).unwrap().unwrap();
        let by_round = dag.vertices_at_round(Round(1)).unwrap().pop().unwrap();
        assert!(Arc::ptr_eq(&by_hash, &by_round));
    }
}
