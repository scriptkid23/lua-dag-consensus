//! Edwards25519 ECVRF, RFC 9381 (placeholder skeleton).
//!
//! The current implementation uses deterministic Ed25519 signatures from
//! `curve25519-dalek` as the proof envelope. This is **not** RFC 9381 —
//! see `docs/superpowers/specs/2026-05-11-folder-architecture-design.md`
//! §12 open question #3. The interface and byte shape (`VrfProof` is 80
//! bytes) match the eventual real ECVRF so call sites are stable.

use curve25519_dalek::{
    constants::ED25519_BASEPOINT_POINT, edwards::CompressedEdwardsY, scalar::Scalar,
};
use rand::RngCore;
use sha2::{Digest, Sha512};
use types::crypto_types::{Hash32, VrfProof};

use crate::error::{Error, Result};

/// VRF keypair.
#[derive(Clone, Debug)]
pub struct VrfKey {
    sk_scalar: Scalar,
    pk_compressed: CompressedEdwardsY,
}

impl VrfKey {
    /// Generate from RNG.
    pub fn random<R: RngCore>(rng: &mut R) -> Self {
        let mut seed = [0u8; 32];
        rng.fill_bytes(&mut seed);
        Self::from_seed(&seed)
    }

    /// Deterministic generation from a 32-byte seed.
    #[must_use]
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        // Wide reduction so any 32-byte seed yields a valid scalar.
        let mut wide = [0u8; 64];
        wide[..32].copy_from_slice(seed);
        let sk_scalar = Scalar::from_bytes_mod_order_wide(&wide);
        let pk_point = ED25519_BASEPOINT_POINT * sk_scalar;
        Self {
            sk_scalar,
            pk_compressed: pk_point.compress(),
        }
    }

    /// Compressed Edwards25519 public point.
    #[must_use]
    pub fn pubkey(&self) -> [u8; 32] {
        self.pk_compressed.to_bytes()
    }
}

/// Produce a VRF proof and its 32-byte output for `alpha`.
///
/// Output layout of the 80-byte proof:
///   bytes  0..32  = gamma (compressed point)
///   bytes 32..48  = c (truncated SHA-512 challenge)
///   bytes 48..80  = s (response scalar)
pub fn vrf_prove(key: &VrfKey, alpha: &[u8]) -> (VrfProof, Hash32) {
    // 1. Hash alpha to a scalar h.
    let h_scalar = hash_to_scalar(b"lua-dag/v1/vrf/h", alpha);
    // 2. gamma = h * sk
    let gamma_point = ED25519_BASEPOINT_POINT * (h_scalar * key.sk_scalar);
    let gamma = gamma_point.compress().to_bytes();
    // 3. k = nonce (deterministic — SHA-512(sk_bytes || alpha))
    let mut hasher = Sha512::new();
    hasher.update(b"lua-dag/v1/vrf/k");
    hasher.update(key.sk_scalar.to_bytes());
    hasher.update(alpha);
    let k_wide: [u8; 64] = hasher.finalize().into();
    let k = Scalar::from_bytes_mod_order_wide(&k_wide);
    // 4. Commitments: U = k*B, V = k*H (skeleton uses U only)
    let u_point = ED25519_BASEPOINT_POINT * k;
    let u_compressed = u_point.compress().to_bytes();
    // 5. c = H(pk || gamma || U)  (truncated to 16 bytes)
    let mut hasher = Sha512::new();
    hasher.update(b"lua-dag/v1/vrf/c");
    hasher.update(key.pubkey());
    hasher.update(gamma);
    hasher.update(u_compressed);
    let c_full: [u8; 64] = hasher.finalize().into();
    let mut c_bytes = [0u8; 16];
    c_bytes.copy_from_slice(&c_full[..16]);
    let mut c_scalar_bytes = [0u8; 32];
    c_scalar_bytes[..16].copy_from_slice(&c_bytes);
    let c_scalar = Scalar::from_bytes_mod_order(c_scalar_bytes);
    // 6. s = k - c * sk
    let s = k - c_scalar * key.sk_scalar;
    let s_bytes = s.to_bytes();
    // 7. Pack proof.
    let mut proof_bytes = [0u8; 80];
    proof_bytes[..32].copy_from_slice(&gamma);
    proof_bytes[32..48].copy_from_slice(&c_bytes);
    proof_bytes[48..80].copy_from_slice(&s_bytes);
    // 8. Output beta = H(gamma).
    let mut hasher = Sha512::new();
    hasher.update(b"lua-dag/v1/vrf/beta");
    hasher.update(gamma);
    let beta_full: [u8; 64] = hasher.finalize().into();
    let mut beta = [0u8; 32];
    beta.copy_from_slice(&beta_full[..32]);

    (VrfProof(proof_bytes), Hash32(beta))
}

/// Verify a VRF proof against `pk_bytes` (compressed Edwards25519 point)
/// and return the 32-byte output on success.
pub fn vrf_verify(pk_bytes: &[u8; 32], alpha: &[u8], proof: &VrfProof) -> Result<Hash32> {
    let pk = CompressedEdwardsY::from_slice(pk_bytes)
        .map_err(|_| Error::VrfVerifyFailed)?
        .decompress()
        .ok_or(Error::VrfVerifyFailed)?;
    let mut gamma_bytes = [0u8; 32];
    gamma_bytes.copy_from_slice(&proof.0[..32]);
    let gamma = CompressedEdwardsY::from_slice(&gamma_bytes)
        .map_err(|_| Error::VrfVerifyFailed)?
        .decompress()
        .ok_or(Error::VrfVerifyFailed)?;
    // Bind gamma to alpha: in vrf_prove gamma = (h_scalar * sk) * B = h_scalar * pk.
    // Without this check the Schnorr step proves only knowledge of sk for *some* gamma.
    let h_scalar = hash_to_scalar(b"lua-dag/v1/vrf/h", alpha);
    if gamma != pk * h_scalar {
        return Err(Error::VrfVerifyFailed);
    }
    let mut c_bytes = [0u8; 16];
    c_bytes.copy_from_slice(&proof.0[32..48]);
    let mut s_bytes = [0u8; 32];
    s_bytes.copy_from_slice(&proof.0[48..80]);
    let s_scalar = Scalar::from_canonical_bytes(s_bytes)
        .into_option()
        .ok_or(Error::VrfVerifyFailed)?;
    let mut c_scalar_bytes = [0u8; 32];
    c_scalar_bytes[..16].copy_from_slice(&c_bytes);
    let c_scalar = Scalar::from_bytes_mod_order(c_scalar_bytes);
    // Reconstruct U = s*B + c*pk.
    let u_recovered = ED25519_BASEPOINT_POINT * s_scalar + pk * c_scalar;
    let u_compressed = u_recovered.compress().to_bytes();
    let mut hasher = Sha512::new();
    hasher.update(b"lua-dag/v1/vrf/c");
    hasher.update(pk_bytes);
    hasher.update(gamma.compress().to_bytes());
    hasher.update(u_compressed);
    let c_full: [u8; 64] = hasher.finalize().into();
    if &c_full[..16] != c_bytes.as_slice() {
        return Err(Error::VrfVerifyFailed);
    }
    let mut hasher = Sha512::new();
    hasher.update(b"lua-dag/v1/vrf/beta");
    hasher.update(gamma.compress().to_bytes());
    let beta_full: [u8; 64] = hasher.finalize().into();
    let mut beta = [0u8; 32];
    beta.copy_from_slice(&beta_full[..32]);
    Ok(Hash32(beta))
}

fn hash_to_scalar(dst: &[u8], data: &[u8]) -> Scalar {
    let mut hasher = Sha512::new();
    hasher.update(dst);
    hasher.update(data);
    let wide: [u8; 64] = hasher.finalize().into();
    Scalar::from_bytes_mod_order_wide(&wide)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha20Rng;

    #[test]
    fn prove_then_verify_succeeds() {
        let mut rng = ChaCha20Rng::from_seed([7; 32]);
        let key = VrfKey::random(&mut rng);
        let (proof, beta) = vrf_prove(&key, b"alpha");
        let beta2 = vrf_verify(&key.pubkey(), b"alpha", &proof).unwrap();
        assert_eq!(beta, beta2);
    }

    #[test]
    fn wrong_alpha_fails() {
        let mut rng = ChaCha20Rng::from_seed([8; 32]);
        let key = VrfKey::random(&mut rng);
        let (proof, _) = vrf_prove(&key, b"alpha");
        let err = vrf_verify(&key.pubkey(), b"different", &proof).unwrap_err();
        assert!(matches!(err, Error::VrfVerifyFailed));
    }

    #[test]
    fn deterministic_proof_for_same_input() {
        let key = VrfKey::from_seed(&[9; 32]);
        let (proof1, beta1) = vrf_prove(&key, b"alpha");
        let (proof2, beta2) = vrf_prove(&key, b"alpha");
        assert_eq!(proof1.0, proof2.0);
        assert_eq!(beta1, beta2);
    }
}
