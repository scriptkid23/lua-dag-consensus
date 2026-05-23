//! Deterministic devnet certified-vertex factory (mirrors `sim::vertex_factory`).

use crypto::hash::{blake3_with_dst, dst};
use types::{
    crypto_types::{BlsAggSig, BlsSig, Hash32},
    dag::{CertifiedVertex, Vertex},
    primitives::{Round, ValidatorId},
    validator::ValidatorSet,
};

/// Distinct certified vertices per round: `2f+1` for `n` validators.
#[must_use]
pub fn quorum_vertex_count(validator_count: u32) -> u32 {
    let f = validator_count.saturating_sub(1) / 3;
    2 * f + 1
}

/// Deterministic vertex hash keyed by round and author.
#[must_use]
pub fn vertex_hash(round: u64, author: &ValidatorId) -> Hash32 {
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

/// Build one certified vertex for a devnet validator at `round`.
#[must_use]
pub fn build_certified_vertex(
    round: u64,
    author: ValidatorId,
    parent_hash: Option<Hash32>,
) -> CertifiedVertex {
    let hash = vertex_hash(round, &author);
    CertifiedVertex {
        vertex: Vertex {
            round: Round(round),
            author,
            parents: parent_hash.into_iter().collect(),
            blobs: vec![],
            hash,
        },
        certificate: fixture_certificate(),
    }
}

/// Build `2f+1` certified vertices for one virtual round from a loaded valset.
///
/// Proposer indices rotate as `(round + i) % n` so each round carries a
/// distinct validator quorum (equal stake), matching sim cadence.
#[must_use]
pub fn build_quorum_vertices_for_valset(
    round: u64,
    valset: &ValidatorSet,
    parent_hash: Option<Hash32>,
) -> Vec<CertifiedVertex> {
    let n = u32::try_from(valset.entries.len()).expect("validator count fits u32");
    let quorum = quorum_vertex_count(n);
    (0..quorum)
        .map(|i| {
            let idx = usize::try_from((round + u64::from(i)) % u64::from(n)).expect("index");
            let author = valset.entries[idx].id;
            build_certified_vertex(round, author, parent_hash)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devnet_keys::devnet_valset_four;

    #[test]
    fn builds_quorum_for_devnet_four() {
        let valset = devnet_valset_four();
        let batch = build_quorum_vertices_for_valset(0, &valset, None);
        assert_eq!(batch.len(), 3);
        assert!(batch
            .iter()
            .all(|v| valset.entries.iter().any(|e| e.id == v.vertex.author)));
    }

    #[test]
    fn sibling_vertices_in_same_round_have_distinct_hashes() {
        let valset = devnet_valset_four();
        let batch = build_quorum_vertices_for_valset(5, &valset, None);
        assert_eq!(batch.len(), 3);
        let h0 = batch[0].vertex.hash;
        let h1 = batch[1].vertex.hash;
        let h2 = batch[2].vertex.hash;
        assert_ne!(h0, h1);
        assert_ne!(h1, h2);
        assert_ne!(h0, h2);
    }
}
