//! Live L1 DAG view: in-memory index + RocksDB vertex persistence (plan 06b-L1).

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use consensus::ports::DagView;
use storage::{db::Database, stores::vertex_store};
use types::{
    crypto_types::Hash32,
    dag::CertifiedVertex,
    primitives::{Round, ValidatorId},
};

/// Thread-safe DAG backed by memory and the `vertex` column family.
#[derive(Debug)]
pub struct LiveDag {
    by_hash: RwLock<HashMap<Hash32, CertifiedVertex>>,
    by_round: RwLock<HashMap<Round, Vec<CertifiedVertex>>>,
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
        let hash = v.vertex.hash;
        let round = v.vertex.round;
        self.by_hash.write().expect("dag hash lock").insert(hash, v.clone());
        self.by_round
            .write()
            .expect("dag round lock")
            .entry(round)
            .or_default()
            .push(v);
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
        if let Some(ref vertex) = v {
            let hash = vertex.vertex.hash;
            self.by_hash
                .write()
                .expect("dag hash lock")
                .insert(hash, vertex.clone());
            self.by_round
                .write()
                .expect("dag round lock")
                .entry(round)
                .or_default()
                .push(vertex.clone());
        }
        Ok(v)
    }
}

impl DagView for LiveDag {
    fn vertex(&self, hash: &Hash32) -> consensus::Result<Option<CertifiedVertex>> {
        if let Some(v) = self.by_hash.read().expect("dag hash lock").get(hash).cloned() {
            return Ok(Some(v));
        }
        Ok(None)
    }

    fn vertices_at_round(&self, round: Round) -> consensus::Result<Vec<CertifiedVertex>> {
        let cached = self
            .by_round
            .read()
            .expect("dag round lock")
            .get(&round)
            .cloned()
            .unwrap_or_default();
        if !cached.is_empty() {
            return Ok(cached);
        }
        Ok(vec![])
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
        assert_eq!(got, v);
        assert_eq!(dag.vertices_at_round(Round(3)).unwrap().len(), 1);
    }
}
