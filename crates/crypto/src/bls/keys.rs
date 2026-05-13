//! BLS12-381 secret keys, public keys, and Proof-of-Possession.
//!
//! Wire shapes live in `types::crypto_types` — this module is responsible
//! for in-memory `blst` objects and the public-key boundary.

use blst::min_pk::{PublicKey as BlstPk, SecretKey as BlstSk, Signature as BlstSig};
use rand::RngCore;
use types::crypto_types::{BlsPubkey, BlsSig, Pop};

use crate::{
    error::{Error, Result},
    hash::dst,
};

/// BLS12-381 secret key wrapper.
///
/// Wraps `blst::min_pk::SecretKey` and zeroises on drop is **not** yet
/// implemented; consumers must keep keys in protected memory until that
/// is added (tracked separately).
#[derive(Clone, Debug)]
pub struct SecretKey(pub(crate) BlstSk);

/// BLS12-381 public key wrapper.
#[derive(Clone, Debug)]
pub struct PublicKey(pub(crate) BlstPk);

impl SecretKey {
    /// Generate a secret key from a 32-byte IKM (input keying material).
    ///
    /// Use [`SecretKey::random`] for ephemeral keys.
    pub fn from_ikm(ikm: &[u8; 32]) -> Result<Self> {
        let sk = BlstSk::key_gen(ikm, &[]).map_err(|_| Error::BlsAggregateFailed("key_gen"))?;
        Ok(Self(sk))
    }

    /// Generate a fresh secret key from the supplied RNG.
    pub fn random<R: RngCore>(rng: &mut R) -> Result<Self> {
        let mut ikm = [0u8; 32];
        rng.fill_bytes(&mut ikm);
        Self::from_ikm(&ikm)
    }

    /// Derive the matching public key.
    #[must_use]
    pub fn public(&self) -> PublicKey {
        PublicKey(self.0.sk_to_pk())
    }
}

impl PublicKey {
    /// Encode to the on-wire 48-byte compressed form.
    #[must_use]
    pub fn to_bytes(&self) -> BlsPubkey {
        BlsPubkey(self.0.compress())
    }

    /// Decode from on-wire 48-byte form. Verifies subgroup membership.
    pub fn from_bytes(b: &BlsPubkey) -> Result<Self> {
        BlstPk::uncompress(&b.0)
            .map(Self)
            .map_err(|_| Error::BlsVerifyFailed)
    }
}

/// Build a Proof-of-Possession for `sk` (`Sign(POP_DST, pk_bytes)`).
pub fn generate_pop(sk: &SecretKey) -> Pop {
    let pk_bytes = sk.public().to_bytes();
    let sig = sk.0.sign(&pk_bytes.0, dst::POP, &[]);
    Pop(BlsSig(sig.compress()))
}

/// Verify a Proof-of-Possession against the claimed public key.
pub fn verify_pop(pk: &PublicKey, pop: &Pop) -> Result<()> {
    let pk_bytes = pk.to_bytes();
    let sig = BlstSig::uncompress(&pop.0.0).map_err(|_| Error::PopInvalid)?;
    let err = sig.verify(true, &pk_bytes.0, dst::POP, &[], &pk.0, true);
    if err == blst::BLST_ERROR::BLST_SUCCESS {
        Ok(())
    } else {
        Err(Error::PopInvalid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha20Rng;

    #[test]
    fn keygen_and_pop_roundtrip() {
        let mut rng = ChaCha20Rng::from_seed([0; 32]);
        let sk = SecretKey::random(&mut rng).unwrap();
        let pk = sk.public();
        let pop = generate_pop(&sk);
        verify_pop(&pk, &pop).expect("PoP must verify");
    }

    #[test]
    fn public_key_round_trips_through_wire_form() {
        let mut rng = ChaCha20Rng::from_seed([1; 32]);
        let sk = SecretKey::random(&mut rng).unwrap();
        let pk = sk.public();
        let bytes = pk.to_bytes();
        let pk2 = PublicKey::from_bytes(&bytes).unwrap();
        assert_eq!(pk.to_bytes(), pk2.to_bytes());
    }
}
