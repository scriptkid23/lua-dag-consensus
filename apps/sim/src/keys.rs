//! Deterministic BLS + ECVRF key material for sim validators.

use consensus::{error::Result, ports::SignerPort};
use crypto::hash::{blake3_with_dst, dst};
use types::crypto_types::{BlsPubkey, BlsSig, Hash32, VrfProof, VrfPubkey};

/// All validator keys derived from one scenario seed.
#[derive(Debug)]
pub struct ValidatorKeyRing {
    bls: Vec<crypto::bls::SecretKey>,
    vrf: Vec<crypto::vrf::VrfKey>,
}

impl ValidatorKeyRing {
    /// Derive `n` independent keys from `seed`.
    #[must_use]
    pub fn from_seed(seed: [u8; 32], n: u32) -> Self {
        let mut bls = Vec::with_capacity(n as usize);
        let mut vrf = Vec::with_capacity(n as usize);
        for i in 0..n {
            let mut label = [0u8; 36];
            label[..32].copy_from_slice(&seed);
            label[32..].copy_from_slice(&i.to_be_bytes());
            let bls_seed = blake3_with_dst(dst::VALIDATOR_BLS_PARTIAL, &label);
            let vrf_seed = blake3_with_dst(dst::MACRO_PROPOSER_SIG, &label);
            bls.push(
                crypto::bls::SecretKey::from_ikm(&bls_seed.0)
                    .expect("sim BLS key derivation must succeed"),
            );
            vrf.push(crypto::vrf::VrfKey::from_seed(&vrf_seed.0));
        }
        Self { bls, vrf }
    }

    /// BLS public key for validator index `i`.
    #[must_use]
    pub fn bls_pubkey(&self, i: usize) -> BlsPubkey {
        self.bls[i].public().to_bytes()
    }

    /// VRF public key for validator index `i`.
    #[must_use]
    pub fn vrf_pubkey(&self, i: usize) -> VrfPubkey {
        VrfPubkey(self.vrf[i].pubkey())
    }

    /// One-validator signing view.
    #[must_use]
    pub fn signer<'a>(&'a self, index: usize) -> ValidatorSigner<'a> {
        ValidatorSigner { ring: self, index }
    }
}

/// Signs as a single validator from a shared key ring.
#[derive(Debug)]
pub struct ValidatorSigner<'a> {
    ring: &'a ValidatorKeyRing,
    index: usize,
}

impl SignerPort for ValidatorSigner<'_> {
    fn sign_bls(&self, dst_tag: &[u8], msg: &[u8]) -> BlsSig {
        crypto::bls::sign::sign(&self.ring.bls[self.index], dst_tag, msg)
    }

    fn vrf_prove(&self, alpha: &[u8]) -> Result<(VrfProof, Hash32)> {
        Ok(crypto::vrf::vrf_prove(
            &self.ring.vrf[self.index],
            alpha,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crypto::hash::dst;

    #[test]
    fn key_ring_is_deterministic() {
        let a = ValidatorKeyRing::from_seed([1; 32], 4);
        let b = ValidatorKeyRing::from_seed([1; 32], 4);
        assert_eq!(a.bls_pubkey(0), b.bls_pubkey(0));
        assert_eq!(a.vrf_pubkey(0), b.vrf_pubkey(0));
    }

    #[test]
    fn signer_produces_verifiable_bls() {
        let ring = ValidatorKeyRing::from_seed([2; 32], 1);
        let signer = ring.signer(0);
        let sig = signer.sign_bls(dst::VALIDATOR_BLS_PARTIAL, b"msg");
        let pk = crypto::bls::PublicKey::from_bytes(&ring.bls_pubkey(0)).unwrap();
        crypto::bls::sign::verify(&pk, dst::VALIDATOR_BLS_PARTIAL, b"msg", &sig).unwrap();
    }
}
