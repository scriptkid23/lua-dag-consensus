//! Causal-set query over LiveDag (07c).

use std::sync::Arc;

use node::{live_dag::LiveDag, query::RocksConsensusQuery};
use storage::{config::StorageConfig, db::Database, RocksPersistence};
use types::{crypto_types::Hash32, dag::Vertex, primitives::Round, ValidatorId};

fn sample_vertex(round: u64, author_byte: u8) -> types::dag::CertifiedVertex {
    types::dag::CertifiedVertex {
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
fn causal_set_returns_hashes_for_round_range() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(
        Database::open(&StorageConfig {
            path: dir.path().to_path_buf(),
            create_if_missing: true,
            max_total_wal_size_mb: 16,
        })
        .unwrap(),
    );
    let live_dag = Arc::new(LiveDag::new(Arc::clone(&db)));
    for round in 0..4 {
        live_dag.ingest(sample_vertex(round, round as u8 + 1)).unwrap();
    }
    let query = RocksConsensusQuery::new(RocksPersistence::new(db), live_dag);
    let hashes = query.causal_set(Round(0), Round(3));
    assert_eq!(hashes.len(), 4);
}
