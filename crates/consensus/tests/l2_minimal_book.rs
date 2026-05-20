//! Unit tests for 03b-1 checkpoint hashing.

use consensus::l2_minimal::checkpoint_hash_from_rounds;
use types::{
    crypto_types::{BlsAggSig, BlsSig, Hash32},
    dag::{CertifiedVertex, Vertex},
    primitives::{Round, ValidatorId},
};

fn fixture_vertex(round: u64, author_byte: u8) -> CertifiedVertex {
    let author = ValidatorId([author_byte; 32]);
    let mut bytes = [0u8; 32];
    bytes[..8].copy_from_slice(&round.to_be_bytes());
    let hash = Hash32(bytes);
    CertifiedVertex {
        vertex: Vertex {
            round: Round(round),
            author,
            parents: vec![],
            blobs: vec![],
            hash,
        },
        certificate: BlsAggSig {
            sig: BlsSig([0; 96]),
            bitmap: vec![],
        },
    }
}

#[test]
fn checkpoint_hash_is_stable_for_same_vertices() {
    let batch = [
        fixture_vertex(0, 0),
        fixture_vertex(1, 1),
        fixture_vertex(2, 2),
        fixture_vertex(3, 3),
    ];
    let h1 = checkpoint_hash_from_rounds(&batch);
    let h2 = checkpoint_hash_from_rounds(&batch);
    assert_eq!(h1, h2);
    assert_ne!(h1, checkpoint_hash_from_rounds(&batch[..3]));
}
