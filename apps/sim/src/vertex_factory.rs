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

/// Distinct certified vertices per sim round: `2f+1` for `n` validators.
#[must_use]
pub fn quorum_vertex_count(validator_count: u32) -> u32 {
    let f = validator_count.saturating_sub(1) / 3;
    2 * f + 1
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

/// Build `2f+1` certified vertices for one virtual round.
///
/// Proposer indices rotate as `(round + i) % n` for `i in 0..=2f` so each
/// round carries a distinct validator quorum (equal stake).
#[must_use]
pub fn build_quorum_vertices_for_round(
    virtual_round: u64,
    validator_count: u32,
    parent_hash: Option<Hash32>,
) -> Vec<CertifiedVertex> {
    let quorum = quorum_vertex_count(validator_count);
    (0..quorum)
        .map(|i| {
            let proposer =
                u32::try_from((virtual_round + u64::from(i)) % u64::from(validator_count))
                    .expect("proposer index fits u32");
            build_certified_vertex(virtual_round, proposer, parent_hash)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quorum_count_is_2f_plus_1() {
        assert_eq!(quorum_vertex_count(4), 3);
        assert_eq!(quorum_vertex_count(7), 5);
        assert_eq!(quorum_vertex_count(1), 1);
    }

    #[test]
    fn sibling_vertices_in_same_round_have_distinct_hashes() {
        let batch = build_quorum_vertices_for_round(5, 4, None);
        assert_eq!(batch.len(), 3);
        let h0 = batch[0].vertex.hash;
        let h1 = batch[1].vertex.hash;
        let h2 = batch[2].vertex.hash;
        assert_ne!(h0, h1);
        assert_ne!(h1, h2);
        assert_ne!(h0, h2);
    }
}
