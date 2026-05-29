//! Deterministic devnet certified-vertex factory (mirrors `sim::vertex_factory`).

use crypto::hash::{blake3_with_dst, dst};
use types::{
    crypto_types::{BlsAggSig, BlsSig, Hash32},
    dag::{BlobRef, CertifiedVertex, Vertex},
    primitives::{Round, ValidatorId},
    validator::ValidatorSet,
};

/// Distinct certified vertices per round: `2f+1` for `n` validators.
#[must_use]
pub fn quorum_vertex_count(validator_count: u32) -> u32 {
    let f = validator_count.saturating_sub(1) / 3;
    2 * f + 1
}

/// Deterministic vertex hash keyed by round and author (legacy sim/devnet fixture path).
#[must_use]
pub fn sim_vertex_hash(round: u64, author: &ValidatorId) -> Hash32 {
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

fn signer_indices_for_round(round: u64, n: u32, quorum: u32) -> Vec<u32> {
    (0..quorum)
        .map(|i| u32::try_from((round + u64::from(i)) % u64::from(n)).expect("index"))
        .collect()
}

/// Build one certified vertex for a devnet validator at `round`.
#[must_use]
pub fn build_certified_vertex(
    round: u64,
    author: ValidatorId,
    parent_hash: Option<Hash32>,
    real_certs: bool,
    valset: &ValidatorSet,
) -> CertifiedVertex {
    build_certified_vertex_with_blobs(
        round,
        author,
        parent_hash,
        real_certs,
        valset,
        vec![],
    )
}

/// Build one certified vertex with optional blob references (07b).
#[must_use]
pub fn build_certified_vertex_with_blobs(
    round: u64,
    author: ValidatorId,
    parent_hash: Option<Hash32>,
    real_certs: bool,
    valset: &ValidatorSet,
    blobs: Vec<BlobRef>,
) -> CertifiedVertex {
    if real_certs {
        let mut vertex = Vertex {
            round: Round(round),
            author,
            parents: parent_hash.into_iter().collect(),
            blobs,
            hash: Hash32([0u8; 32]),
        };
        dag::signing::seal_hash(&mut vertex);
        let n = u32::try_from(valset.entries.len()).expect("validator count fits u32");
        let quorum = quorum_vertex_count(n);
        let indices = signer_indices_for_round(round, n, quorum);
        return dag::cert::build_quorum_cert(&vertex, valset, &indices)
            .expect("devnet quorum cert must build");
    }

    let hash = sim_vertex_hash(round, &author);
    CertifiedVertex {
        vertex: Vertex {
            round: Round(round),
            author,
            parents: parent_hash.into_iter().collect(),
            blobs,
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
    real_certs: bool,
) -> Vec<CertifiedVertex> {
    build_quorum_vertices_with_blobs(round, valset, parent_hash, real_certs, vec![])
}

/// Build `2f+1` certified vertices, partitioning `blobs` round-robin across
/// quorum slots (slot `i` receives blob `j` where `j % quorum == i`).
#[must_use]
pub fn build_quorum_vertices_with_blobs(
    round: u64,
    valset: &ValidatorSet,
    parent_hash: Option<Hash32>,
    real_certs: bool,
    blobs: Vec<BlobRef>,
) -> Vec<CertifiedVertex> {
    let n = u32::try_from(valset.entries.len()).expect("validator count fits u32");
    let quorum = quorum_vertex_count(n);
    let quorum_usize = quorum as usize;

    let mut buckets: Vec<Vec<BlobRef>> = (0..quorum_usize).map(|_| Vec::new()).collect();
    for (j, b) in blobs.into_iter().enumerate() {
        buckets[j % quorum_usize].push(b);
    }

    (0..quorum)
        .map(|i| {
            let idx = usize::try_from((round + u64::from(i)) % u64::from(n)).expect("index");
            let author = valset.entries[idx].id;
            build_certified_vertex_with_blobs(
                round,
                author,
                parent_hash,
                real_certs,
                valset,
                std::mem::take(&mut buckets[i as usize]),
            )
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
        let batch = build_quorum_vertices_for_valset(0, &valset, None, false);
        assert_eq!(batch.len(), 3);
        assert!(batch
            .iter()
            .all(|v| valset.entries.iter().any(|e| e.id == v.vertex.author)));
    }

    #[test]
    fn real_certs_verify_against_devnet_valset() {
        let valset = devnet_valset_four();
        let batch = build_quorum_vertices_for_valset(0, &valset, None, true);
        assert_eq!(batch.len(), 3);
        for cv in &batch {
            dag::cert::verify_certified_vertex(cv, &valset).expect("real cert must verify");
        }
    }

    #[test]
    fn sibling_vertices_in_same_round_have_distinct_hashes() {
        let valset = devnet_valset_four();
        let batch = build_quorum_vertices_for_valset(5, &valset, None, false);
        assert_eq!(batch.len(), 3);
        let h0 = batch[0].vertex.hash;
        let h1 = batch[1].vertex.hash;
        let h2 = batch[2].vertex.hash;
        assert_ne!(h0, h1);
        assert_ne!(h1, h2);
        assert_ne!(h0, h2);
    }

    #[test]
    fn blobs_partition_round_robin_across_quorum_slots() {
        use types::crypto_types::Hash32;
        let valset = devnet_valset_four();
        let mk = |tag: u8| BlobRef {
            blob_id: types::primitives::BlobId([tag; 32]),
            commitment: Hash32([tag; 32]),
            size_bytes: u64::from(tag) * 100,
        };
        let blobs = vec![mk(1), mk(2), mk(3), mk(4), mk(5)];
        let batch = build_quorum_vertices_with_blobs(7, &valset, None, false, blobs);
        assert_eq!(batch.len(), 3);
        // j % 3: 0,1,2,0,1 → slot0=[1,4], slot1=[2,5], slot2=[3].
        assert_eq!(batch[0].vertex.blobs.len(), 2);
        assert_eq!(batch[0].vertex.blobs[0].blob_id.0[0], 1);
        assert_eq!(batch[0].vertex.blobs[1].blob_id.0[0], 4);
        assert_eq!(batch[1].vertex.blobs.len(), 2);
        assert_eq!(batch[1].vertex.blobs[0].blob_id.0[0], 2);
        assert_eq!(batch[1].vertex.blobs[1].blob_id.0[0], 5);
        assert_eq!(batch[2].vertex.blobs.len(), 1);
        assert_eq!(batch[2].vertex.blobs[0].blob_id.0[0], 3);
    }

    #[test]
    fn empty_blob_list_yields_empty_buckets_for_all_authors() {
        let valset = devnet_valset_four();
        let batch = build_quorum_vertices_with_blobs(0, &valset, None, false, vec![]);
        assert_eq!(batch.len(), 3);
        assert!(batch.iter().all(|cv| cv.vertex.blobs.is_empty()));
    }
}
