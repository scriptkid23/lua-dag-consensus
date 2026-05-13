//! BLS aggregate signature helpers. Uses `blst::min_pk::AggregateSignature`.

use blst::min_pk::{AggregatePublicKey, AggregateSignature, Signature as BlstSig};
use types::crypto_types::BlsSig;

use super::keys::PublicKey;
use crate::error::{Error, Result};

/// Aggregate a slice of signatures into one compressed signature.
pub fn aggregate_sigs(sigs: &[BlsSig]) -> Result<BlsSig> {
    if sigs.is_empty() {
        return Err(Error::BlsAggregateFailed("empty signature set"));
    }
    let parsed: Vec<BlstSig> = sigs
        .iter()
        .map(|s| BlstSig::uncompress(&s.0).map_err(|_| Error::BlsAggregateFailed("invalid sig")))
        .collect::<Result<_>>()?;
    let refs: Vec<&BlstSig> = parsed.iter().collect();
    let agg = AggregateSignature::aggregate(&refs, true)
        .map_err(|_| Error::BlsAggregateFailed("aggregate"))?;
    Ok(BlsSig(agg.to_signature().compress()))
}

/// Verify that `agg` is the aggregate signature of `pks` over the same
/// `msg` under `dst`.
pub fn verify_aggregate(pks: &[PublicKey], dst: &[u8], msg: &[u8], agg: &BlsSig) -> Result<()> {
    if pks.is_empty() {
        return Err(Error::BlsAggregateFailed("empty pubkey set"));
    }
    let agg_sig = BlstSig::uncompress(&agg.0).map_err(|_| Error::BlsVerifyFailed)?;
    let pk_refs: Vec<&blst::min_pk::PublicKey> = pks.iter().map(|p| &p.0).collect();
    let agg_pk = AggregatePublicKey::aggregate(&pk_refs, true)
        .map_err(|_| Error::BlsAggregateFailed("aggregate pk"))?;
    let err = agg_sig.verify(true, msg, dst, &[], &agg_pk.to_public_key(), true);
    if err == blst::BLST_ERROR::BLST_SUCCESS {
        Ok(())
    } else {
        Err(Error::BlsVerifyFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bls::{keys::SecretKey, sign::sign};
    use crate::hash::dst as dsts;
    use rand::SeedableRng;
    use rand_chacha::ChaCha20Rng;

    #[test]
    fn aggregate_three_sigs_over_same_message() {
        let mut rng = ChaCha20Rng::from_seed([5; 32]);
        let sks: Vec<_> = (0..3)
            .map(|_| SecretKey::random(&mut rng).unwrap())
            .collect();
        let pks: Vec<_> = sks.iter().map(SecretKey::public).collect();
        let msg = b"shared-message";
        let sigs: Vec<_> = sks.iter().map(|sk| sign(sk, dsts::MICRO_QC, msg)).collect();

        let agg = aggregate_sigs(&sigs).unwrap();
        verify_aggregate(&pks, dsts::MICRO_QC, msg, &agg).unwrap();
    }

    #[test]
    fn aggregate_with_wrong_pks_fails() {
        let mut rng = ChaCha20Rng::from_seed([6; 32]);
        let sks: Vec<_> = (0..3)
            .map(|_| SecretKey::random(&mut rng).unwrap())
            .collect();
        let other = SecretKey::random(&mut rng).unwrap();
        let msg = b"m";
        let sigs: Vec<_> = sks.iter().map(|sk| sign(sk, dsts::MICRO_QC, msg)).collect();
        let agg = aggregate_sigs(&sigs).unwrap();

        let mut wrong_pks: Vec<_> = sks.iter().map(SecretKey::public).collect();
        wrong_pks[0] = other.public();
        let err = verify_aggregate(&wrong_pks, dsts::MICRO_QC, msg, &agg).unwrap_err();
        assert!(matches!(err, Error::BlsVerifyFailed));
    }
}
