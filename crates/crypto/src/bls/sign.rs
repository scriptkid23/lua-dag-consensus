//! Single-key BLS sign/verify. Aggregation is in `aggregate.rs`.

use blst::min_pk::Signature as BlstSig;
use types::crypto_types::BlsSig;

use super::keys::{PublicKey, SecretKey};
use crate::error::{Error, Result};

/// Sign `msg` with `sk` under the supplied DST.
#[must_use]
pub fn sign(sk: &SecretKey, dst: &[u8], msg: &[u8]) -> BlsSig {
    let sig = sk.0.sign(msg, dst, &[]);
    BlsSig(sig.compress())
}

/// Verify a single signature under DST.
pub fn verify(pk: &PublicKey, dst: &[u8], msg: &[u8], sig: &BlsSig) -> Result<()> {
    let s = BlstSig::uncompress(&sig.0).map_err(|_| Error::BlsVerifyFailed)?;
    let err = s.verify(true, msg, dst, &[], &pk.0, true);
    if err == blst::BLST_ERROR::BLST_SUCCESS {
        Ok(())
    } else {
        Err(Error::BlsVerifyFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bls::keys::SecretKey;
    use crate::hash::dst as dsts;
    use rand::SeedableRng;
    use rand_chacha::ChaCha20Rng;

    #[test]
    fn sign_then_verify_succeeds() {
        let mut rng = ChaCha20Rng::from_seed([2; 32]);
        let sk = SecretKey::random(&mut rng).unwrap();
        let pk = sk.public();
        let sig = sign(&sk, dsts::MICRO_QC, b"hello");
        verify(&pk, dsts::MICRO_QC, b"hello", &sig).unwrap();
    }

    #[test]
    fn wrong_message_fails() {
        let mut rng = ChaCha20Rng::from_seed([3; 32]);
        let sk = SecretKey::random(&mut rng).unwrap();
        let pk = sk.public();
        let sig = sign(&sk, dsts::MICRO_QC, b"hello");
        let err = verify(&pk, dsts::MICRO_QC, b"goodbye", &sig).unwrap_err();
        assert!(matches!(err, Error::BlsVerifyFailed));
    }

    #[test]
    fn wrong_dst_fails() {
        let mut rng = ChaCha20Rng::from_seed([4; 32]);
        let sk = SecretKey::random(&mut rng).unwrap();
        let pk = sk.public();
        let sig = sign(&sk, dsts::MICRO_QC, b"hello");
        let err = verify(&pk, dsts::MACRO_VOTE, b"hello", &sig).unwrap_err();
        assert!(matches!(err, Error::BlsVerifyFailed));
    }
}
