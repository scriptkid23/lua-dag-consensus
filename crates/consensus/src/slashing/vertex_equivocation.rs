//! L1 vertex double-propose detector (100 % slash, 06-04 design).

use crypto::{bls::PublicKey, bls::sign::verify as bls_verify, hash::dst};
use types::{
    dag::VertexProposal,
    primitives::ValidatorId,
    slashing::VertexEquivocation,
    validator::ValidatorSet,
};

use crate::error::Result;

/// Verify a vertex-equivocation evidence bundle.
///
/// Valid evidence = same author, same round, different content hash,
/// both proposer signatures valid under [`dst::VERTEX_PROPOSAL`] over
/// [`dag::signing::signing_bytes`].
pub fn verify(ev: &VertexEquivocation, set: &ValidatorSet) -> Result<()> {
    let entry = set
        .entries
        .iter()
        .find(|e| e.id == ev.validator)
        .ok_or_else(|| crate::Error::InvalidConfig("unknown validator".into()))?;
    let pk = PublicKey::from_bytes(&entry.bls_pubkey)
        .map_err(|_| crate::Error::InvalidConfig("invalid bls pubkey".into()))?;

    if ev.a.0.author != ev.validator || ev.b.0.author != ev.validator {
        return Err(crate::Error::InvalidConfig(
            "equivocation vertices must be authored by the offender".into(),
        ));
    }
    if ev.a.0.round != ev.b.0.round || ev.a.0.hash == ev.b.0.hash {
        return Err(crate::Error::InvalidConfig(
            "equivocation vertices must share round and differ in hash".into(),
        ));
    }

    for (vertex, sig) in [&ev.a, &ev.b] {
        let msg = dag::signing::signing_bytes(vertex);
        bls_verify(&pk, dst::VERTEX_PROPOSAL, &msg, sig)
            .map_err(|_| crate::Error::InvalidConfig("invalid proposer sig".into()))?;
    }
    Ok(())
}

/// Build evidence from two conflicting proposals at the same round.
#[must_use]
pub fn detect(
    validator: ValidatorId,
    a: VertexProposal,
    b: VertexProposal,
) -> VertexEquivocation {
    VertexEquivocation::from_proposals(validator, a, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crypto::bls::keys::SecretKey;
    use crypto::bls::sign::sign;
    use types::{
        crypto_types::{Hash32, VrfPubkey},
        dag::Vertex,
        primitives::{Epoch, Round, StakeWeight},
        validator::{ValidatorEntry, ValidatorIdentity},
    };

    fn sk() -> SecretKey {
        SecretKey::from_ikm(&[0x11; 32]).unwrap()
    }

    fn set_with(id: ValidatorId) -> ValidatorSet {
        ValidatorSet {
            epoch: Epoch(0),
            entries: vec![ValidatorEntry {
                id,
                bls_pubkey: sk().public().to_bytes(),
                vrf_pubkey: VrfPubkey::zero(),
                stake: StakeWeight(1),
                identity: ValidatorIdentity {
                    asn: None,
                    cloud: None,
                    region: None,
                },
            }],
            total_stake: StakeWeight(1),
        }
    }

    fn signed_proposal(author: ValidatorId, parent: u8) -> VertexProposal {
        let mut vertex = Vertex {
            round: Round(2),
            author,
            parents: vec![Hash32([parent; 32])],
            blobs: vec![],
            hash: Hash32::zero(),
        };
        dag::signing::seal_hash(&mut vertex);
        let sig = sign(
            &sk(),
            dst::VERTEX_PROPOSAL,
            &dag::signing::signing_bytes(&vertex),
        );
        VertexProposal {
            vertex,
            proposer_sig: sig,
        }
    }

    #[test]
    fn valid_double_propose_evidence_verifies() {
        let author = ValidatorId([1; 32]);
        let ev = detect(author, signed_proposal(author, 1), signed_proposal(author, 2));
        verify(&ev, &set_with(author)).expect("valid evidence");
    }

    #[test]
    fn same_hash_pair_is_rejected() {
        let author = ValidatorId([1; 32]);
        let p = signed_proposal(author, 1);
        let ev = detect(author, p.clone(), p);
        assert!(verify(&ev, &set_with(author)).is_err());
    }

    #[test]
    fn forged_sig_is_rejected() {
        let author = ValidatorId([1; 32]);
        let mut bad = signed_proposal(author, 2);
        bad.proposer_sig = types::crypto_types::BlsSig([0xEE; 96]);
        let ev = detect(author, signed_proposal(author, 1), bad);
        assert!(verify(&ev, &set_with(author)).is_err());
    }
}
