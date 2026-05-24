//! Deterministic certified-vertex factory for the simulator.

use crypto::hash::{blake3_with_dst, dst};
use dag::cert;
use types::{
    crypto_types::Hash32,
    dag::{CertifiedVertex, Vertex},
    primitives::{Round, ValidatorId},
    validator::ValidatorSet,
};

use crate::keys::ValidatorKeyRing;

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

fn sim_vertex_hash(round: u64, author: &ValidatorId) -> Hash32 {
    let mut buf = Vec::with_capacity(8 + 32);
    buf.extend_from_slice(&round.to_be_bytes());
    buf.extend_from_slice(author.as_bytes());
    blake3_with_dst(dst::SIM_VERTEX_HASH, &buf)
}

fn signer_indices_for_round(round: u64, n: u32, quorum: u32) -> Vec<u32> {
    (0..quorum)
        .map(|i| u32::try_from((round + u64::from(i)) % u64::from(n)).expect("index"))
        .collect()
}

/// Build a deterministic certified vertex for sim ticks.
///
/// Uses real BLS quorum certificates from `dag::cert` but keeps the sim-only
/// [`dst::SIM_VERTEX_HASH`] recipe for `vertex.hash` so Bullshark / replay
/// tests stay stable. Sim does not run `verify_certified_vertex`.
#[must_use]
pub fn build_certified_vertex(
    virtual_round: u64,
    proposer_index: u32,
    parent_hash: Option<Hash32>,
    valset: &ValidatorSet,
    key_ring: &ValidatorKeyRing,
) -> CertifiedVertex {
    let author = validator_id_for_index(proposer_index);
    let vertex = Vertex {
        round: Round(virtual_round),
        author,
        parents: parent_hash.into_iter().collect(),
        blobs: vec![],
        hash: Hash32([0u8; 32]),
    };
    let n = u32::try_from(valset.entries.len()).expect("validator count fits u32");
    let quorum = quorum_vertex_count(n);
    let indices = signer_indices_for_round(virtual_round, n, quorum);
    let mut cv = cert::build_quorum_cert_with(&vertex, valset, &indices, |idx| {
        Ok(key_ring.bls_secret(idx as usize))
    })
    .expect("sim quorum cert must build");
    cv.vertex.hash = sim_vertex_hash(virtual_round, &author);
    cv
}

/// Build `2f+1` certified vertices for one virtual round.
///
/// Proposer indices rotate as `(round + i) % n` for `i in 0..=2f` so each
/// round carries a distinct validator quorum (equal stake).
#[must_use]
pub fn build_quorum_vertices_for_round(
    virtual_round: u64,
    valset: &ValidatorSet,
    parent_hash: Option<Hash32>,
    key_ring: &ValidatorKeyRing,
) -> Vec<CertifiedVertex> {
    let n = u32::try_from(valset.entries.len()).expect("validator count fits u32");
    let quorum = quorum_vertex_count(n);
    (0..quorum)
        .map(|i| {
            let proposer =
                u32::try_from((virtual_round + u64::from(i)) % u64::from(n)).expect("proposer index");
            build_certified_vertex(virtual_round, proposer, parent_hash, valset, key_ring)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::primitives::{Epoch, StakeWeight};
    use types::validator::{ValidatorEntry, ValidatorIdentity};

    fn sim_valset_and_keys(n: u32) -> (ValidatorSet, ValidatorKeyRing) {
        let key_ring = ValidatorKeyRing::from_seed([9; 32], n);
        let mut entries = Vec::with_capacity(n as usize);
        for i in 0..n {
            entries.push(ValidatorEntry {
                id: validator_id_for_index(i),
                bls_pubkey: key_ring.bls_pubkey(i as usize),
                vrf_pubkey: key_ring.vrf_pubkey(i as usize),
                stake: StakeWeight(1_000),
                identity: ValidatorIdentity {
                    asn: None,
                    cloud: None,
                    region: None,
                },
            });
        }
        let set = ValidatorSet {
            epoch: Epoch(0),
            entries,
            total_stake: StakeWeight(u64::from(n) * 1_000),
        };
        (set, key_ring)
    }

    #[test]
    fn quorum_count_is_2f_plus_1() {
        assert_eq!(quorum_vertex_count(4), 3);
        assert_eq!(quorum_vertex_count(7), 5);
        assert_eq!(quorum_vertex_count(1), 1);
    }

    #[test]
    fn sibling_vertices_in_same_round_have_distinct_hashes() {
        let (valset, key_ring) = sim_valset_and_keys(4);
        let batch = build_quorum_vertices_for_round(5, &valset, None, &key_ring);
        assert_eq!(batch.len(), 3);
        let h0 = batch[0].vertex.hash;
        let h1 = batch[1].vertex.hash;
        let h2 = batch[2].vertex.hash;
        assert_ne!(h0, h1);
        assert_ne!(h1, h2);
        assert_ne!(h0, h2);
    }

    #[test]
    fn certs_are_not_fixture_bytes() {
        let (valset, key_ring) = sim_valset_and_keys(4);
        let cv = build_certified_vertex(0, 0, None, &valset, &key_ring);
        assert_ne!(cv.certificate.sig.0, [0xAB; 96]);
    }
}
