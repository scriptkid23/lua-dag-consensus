//! Deterministic certified-vertex factory for the simulator.

use crypto::hash::{blake3_with_dst, dst};
use types::{
    crypto_types::{BlsAggSig, BlsSig, Hash32},
    dag::{CertifiedVertex, Vertex},
    primitives::{Round, ValidatorId},
};

/// Map a validator index to a deterministic `ValidatorId`.
#[must_use]
pub fn validator_id_for_index(index: u32) -> ValidatorId {
    let mut id = [0u8; 32];
    id[..4].copy_from_slice(&index.to_be_bytes());
    ValidatorId(id)
}

fn vertex_hash(round: u64, author: &ValidatorId) -> Hash32 {
    let mut buf = Vec::with_capacity(8 + 32);
    buf.extend_from_slice(&round.to_be_bytes());
    buf.extend_from_slice(author.as_bytes());
    blake3_with_dst(dst::SIM_VERTEX_HASH, &buf)
}

fn fixture_certificate() -> BlsAggSig {
    BlsAggSig {
        sig: BlsSig([0xAB; 96]),
        bitmap: vec![0xFF],
    }
}

/// Build a deterministic certified vertex for sim ticks.
#[must_use]
pub fn build_certified_vertex(
    virtual_round: u64,
    proposer_index: u32,
    parent_hash: Option<Hash32>,
) -> CertifiedVertex {
    let author = validator_id_for_index(proposer_index);
    let hash = vertex_hash(virtual_round, &author);
    CertifiedVertex {
        vertex: Vertex {
            round: Round(virtual_round),
            author,
            parents: parent_hash.into_iter().collect(),
            blobs: vec![],
            hash,
        },
        certificate: fixture_certificate(),
    }
}
