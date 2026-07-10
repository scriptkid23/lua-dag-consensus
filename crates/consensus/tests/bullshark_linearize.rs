//! BFS closure over the anchor's causal past.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use consensus::{
    bullshark::linearize::{Linearization, checkpoint_hash_from_linearization},
    ports::DagView,
};
use types::{
    crypto_types::{BlsAggSig, BlsSig, Hash32},
    dag::{CertifiedVertex, SharedCertifiedVertex, Vertex},
    primitives::{Round, ValidatorId},
};

#[derive(Default)]
struct HashMapDag {
    by_hash: RwLock<HashMap<Hash32, SharedCertifiedVertex>>,
}

impl HashMapDag {
    fn insert(&self, v: CertifiedVertex) {
        let shared = Arc::new(v);
        self.by_hash
            .write()
            .unwrap()
            .insert(shared.vertex.hash, shared);
    }
}

impl DagView for HashMapDag {
    fn vertex(&self, hash: &Hash32) -> consensus::Result<Option<SharedCertifiedVertex>> {
        Ok(self.by_hash.read().unwrap().get(hash).cloned())
    }

    fn vertices_at_round(&self, _round: Round) -> consensus::Result<Vec<SharedCertifiedVertex>> {
        Ok(vec![])
    }
}

fn fixture_vertex(hash_byte: u8, round: u64, parents: Vec<Hash32>) -> CertifiedVertex {
    CertifiedVertex {
        vertex: Vertex {
            round: Round(round),
            author: ValidatorId([hash_byte; 32]),
            parents,
            blobs: vec![],
            hash: Hash32([hash_byte; 32]),
        },
        certificate: BlsAggSig {
            sig: BlsSig([0; 96]),
            bitmap: vec![],
        },
    }
}

fn fixture_dag_diamond() -> (HashMapDag, Hash32) {
    let dag = HashMapDag::default();
    let leaf_a = fixture_vertex(0x10, 0, vec![]);
    let leaf_b = fixture_vertex(0x20, 0, vec![]);
    let mid_c = fixture_vertex(0x30, 1, vec![leaf_a.vertex.hash, leaf_b.vertex.hash]);
    let mid_d = fixture_vertex(0x40, 1, vec![leaf_a.vertex.hash]);
    let anchor = fixture_vertex(0x50, 2, vec![mid_c.vertex.hash, mid_d.vertex.hash]);
    let anchor_hash = anchor.vertex.hash;
    dag.insert(leaf_a);
    dag.insert(leaf_b);
    dag.insert(mid_c);
    dag.insert(mid_d);
    dag.insert(anchor);
    (dag, anchor_hash)
}

#[test]
fn closure_visits_in_bfs_order_with_hash_tiebreak() {
    let (dag, anchor_hash) = fixture_dag_diamond();
    let lin = Linearization::closure_of_anchor(anchor_hash, &dag).unwrap();
    // Anchor first, then anchor's parents sorted by hash (mid_c=0x30, mid_d=0x40),
    // then unique grandparents sorted by hash (leaf_a=0x10, leaf_b=0x20).
    assert_eq!(
        lin.order,
        vec![
            Hash32([0x50; 32]),
            Hash32([0x30; 32]),
            Hash32([0x40; 32]),
            Hash32([0x10; 32]),
            Hash32([0x20; 32]),
        ],
    );
}

#[test]
fn closure_is_deterministic_on_replay() {
    let (dag, anchor_hash) = fixture_dag_diamond();
    let a = Linearization::closure_of_anchor(anchor_hash, &dag).unwrap();
    let b = Linearization::closure_of_anchor(anchor_hash, &dag).unwrap();
    assert_eq!(a, b);
    let h1 = checkpoint_hash_from_linearization(&a);
    let h2 = checkpoint_hash_from_linearization(&b);
    assert_eq!(h1, h2);
}

#[test]
fn closure_is_cycle_safe() {
    let dag = HashMapDag::default();
    // Construct a (illegal but defensive) cycle: A -> B -> A.
    let hash_a = Hash32([0xAA; 32]);
    let hash_b = Hash32([0xBB; 32]);
    let a = CertifiedVertex {
        vertex: Vertex {
            round: Round(0),
            author: ValidatorId([0; 32]),
            parents: vec![hash_b],
            blobs: vec![],
            hash: hash_a,
        },
        certificate: BlsAggSig {
            sig: BlsSig([0; 96]),
            bitmap: vec![],
        },
    };
    let b = CertifiedVertex {
        vertex: Vertex {
            round: Round(0),
            author: ValidatorId([1; 32]),
            parents: vec![hash_a],
            blobs: vec![],
            hash: hash_b,
        },
        certificate: BlsAggSig {
            sig: BlsSig([0; 96]),
            bitmap: vec![],
        },
    };
    dag.insert(a);
    dag.insert(b);
    let lin = Linearization::closure_of_anchor(hash_a, &dag).unwrap();
    assert_eq!(lin.order, vec![hash_a, hash_b]);
}

#[test]
fn missing_anchor_yields_singleton_order() {
    let dag = HashMapDag::default();
    let missing = Hash32([0xFF; 32]);
    let lin = Linearization::closure_of_anchor(missing, &dag).unwrap();
    assert_eq!(lin.order, vec![missing]);
}
