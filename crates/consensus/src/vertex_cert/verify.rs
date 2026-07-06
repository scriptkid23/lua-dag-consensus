//! Pure signature checks for vertex proposals and partials.

use crypto::{bls::PublicKey, bls::sign::verify, hash::dst};
use types::{
    dag::{Vertex, VertexPartial, VertexProposal},
    primitives::ValidatorId,
    validator::{ValidatorEntry, ValidatorSet},
};

fn entry_for<'a>(set: &'a ValidatorSet, id: &ValidatorId) -> Option<&'a ValidatorEntry> {
    set.entries.iter().find(|e| &e.id == id)
}

fn pk_for(set: &ValidatorSet, id: &ValidatorId) -> Option<PublicKey> {
    PublicKey::from_bytes(&entry_for(set, id)?.bls_pubkey).ok()
}

/// Verify a proposal's authority signature: author ∈ valset and
/// `proposer_sig` valid under [`dst::VERTEX_PROPOSAL`] over
/// `signing_bytes(vertex)`. Hash integrity is checked by the caller.
#[must_use]
pub fn verify_proposal(set: &ValidatorSet, p: &VertexProposal) -> bool {
    let Some(pk) = pk_for(set, &p.vertex.author) else {
        return false;
    };
    let msg = dag::signing::signing_bytes(&p.vertex);
    verify(&pk, dst::VERTEX_PROPOSAL, &msg, &p.proposer_sig).is_ok()
}

/// Verify a partial vote against the proposal's vertex: routing fields
/// match, voter ∈ valset, and `sig` valid under [`dst::VERTEX_CERT`]
/// over `signing_bytes(vertex)`.
#[must_use]
pub fn verify_partial(set: &ValidatorSet, bp: &VertexPartial, vertex: &Vertex) -> bool {
    if bp.vertex_hash != vertex.hash || bp.round != vertex.round || bp.author != vertex.author {
        return false;
    }
    let Some(pk) = pk_for(set, &bp.voter) else {
        return false;
    };
    let msg = dag::signing::signing_bytes(vertex);
    verify(&pk, dst::VERTEX_CERT, &msg, &bp.sig).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vertex_cert::test_fixture::Ring;
    use crypto::hash::dst;
    use types::{crypto_types::Hash32, primitives::Round};

    fn sealed(ring: &Ring, author: u8) -> Vertex {
        let mut v = Vertex {
            round: Round(0),
            author: ring.id(author),
            parents: vec![],
            blobs: vec![],
            hash: Hash32::zero(),
        };
        dag::signing::seal_hash(&mut v);
        v
    }

    #[test]
    fn proposal_sig_verifies_and_forgery_fails() {
        let ring = Ring::new(4);
        let vertex = sealed(&ring, 0);
        let msg = dag::signing::signing_bytes(&vertex);
        let good = VertexProposal {
            vertex: vertex.clone(),
            proposer_sig: ring.sign(0, dst::VERTEX_PROPOSAL, &msg),
        };
        assert!(verify_proposal(&ring.set, &good));
        // signed by the wrong validator
        let forged = VertexProposal {
            vertex,
            proposer_sig: ring.sign(1, dst::VERTEX_PROPOSAL, &msg),
        };
        assert!(!verify_proposal(&ring.set, &forged));
    }

    #[test]
    fn partial_verifies_and_field_mismatch_fails() {
        let ring = Ring::new(4);
        let vertex = sealed(&ring, 0);
        let msg = dag::signing::signing_bytes(&vertex);
        let good = VertexPartial {
            vertex_hash: vertex.hash,
            round: vertex.round,
            author: vertex.author,
            voter: ring.id(1),
            sig: ring.sign(1, dst::VERTEX_CERT, &msg),
        };
        assert!(verify_partial(&ring.set, &good, &vertex));
        let mut wrong_round = good.clone();
        wrong_round.round = Round(9);
        assert!(!verify_partial(&ring.set, &wrong_round, &vertex));
        let mut unknown_voter = good;
        unknown_voter.voter = types::primitives::ValidatorId([0xEE; 32]);
        assert!(!verify_partial(&ring.set, &unknown_voter, &vertex));
    }
}
