//! Edwards25519 ECVRF per RFC 9381 (`ECVRF-EDWARDS25519-SHA512-TAI`).
//!
//! In-tree implementation using only `curve25519-dalek` and `sha2`.
//! Public API and 80-byte proof shape are stable for downstream call sites.

mod rfc9381;

use curve25519_dalek::{
    constants::ED25519_BASEPOINT_POINT,
    edwards::CompressedEdwardsY,
    scalar::Scalar,
};
use rand::RngCore;
use rfc9381::{
    decompress_pubkey, expand_secret_key, gamma_to_hash32, parse_proof, prove_internal,
    verify_internal,
};
use types::crypto_types::{Hash32, VrfProof};

use crate::error::{Error, Result};

/// VRF keypair (Ed25519 secret seed + derived public point).
#[derive(Clone, Debug)]
pub struct VrfKey {
    sk_scalar: Scalar,
    nonce: [u8; 32],
    pk_compressed: CompressedEdwardsY,
}

impl VrfKey {
    /// Generate from RNG.
    pub fn random<R: RngCore>(rng: &mut R) -> Self {
        let mut seed = [0u8; 32];
        rng.fill_bytes(&mut seed);
        Self::from_seed(&seed)
    }

    /// Deterministic generation from a 32-byte Ed25519 seed (RFC 8032).
    #[must_use]
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let (sk_scalar, nonce) = expand_secret_key(seed);
        let pk_point = ED25519_BASEPOINT_POINT * sk_scalar;
        Self {
            sk_scalar,
            nonce,
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
/// Proof layout (80 bytes, RFC 9381):
///   bytes  0..32  = Gamma (compressed point)
///   bytes 32..48  = c (challenge, truncated to 16 bytes)
///   bytes 48..80  = s (response scalar)
pub fn vrf_prove(key: &VrfKey, alpha: &[u8]) -> (VrfProof, Hash32) {
    let pk_bytes = key.pubkey();
    let proof_bytes = prove_internal(&key.sk_scalar, &key.nonce, &pk_bytes, alpha);
    let parsed = parse_proof(&proof_bytes).expect("locally generated proof must parse");
    let beta = gamma_to_hash32(&parsed.gamma);
    (VrfProof(proof_bytes), Hash32(beta))
}

/// Verify a VRF proof against `pk_bytes` and return the 32-byte output on success.
pub fn vrf_verify(pk_bytes: &[u8; 32], alpha: &[u8], proof: &VrfProof) -> Result<Hash32> {
    let pk_point = decompress_pubkey(pk_bytes).ok_or(Error::VrfVerifyFailed)?;
    let parsed = parse_proof(&proof.0).ok_or(Error::VrfVerifyFailed)?;
    if !verify_internal(&pk_point, pk_bytes, alpha, &parsed) {
        return Err(Error::VrfVerifyFailed);
    }
    Ok(Hash32(gamma_to_hash32(&parsed.gamma)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha20Rng;

    use curve25519_dalek::traits::Identity;
    use rfc9381::{encode_to_curve, expand_secret_key, hash_points, scalar_from_nonce};

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

    /// RFC 9381 Appendix B.3 — Example 16.
    #[test]
    fn rfc9381_example_16() {
        assert_rfc9381_vector(Rfc9381Vector {
            sk: hex32("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60"),
            pk: hex32("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"),
            alpha: b"",
            h: hex32("91bbed02a99461df1ad4c6564a5f5d829d0b90cfc7903e7a5797bd658abf3318"),
            pi: hex80("8657106690b5526245a92b003bb079ccd1a92130477671f6fc01ad16f26f723f26f8a57ccaed74ee1b190bed1f479d9727d2d0f9b005a6e456a35d4fb0daab1268a1b0db10836d9826a528ca76567805"),
            beta_prefix: hex32("90cf1df3b703cce59e2a35b925d411164068269d7b2d29f3301c03dd757876ff"),
        });
    }

    /// RFC 9381 Appendix B.3 — Example 17.
    #[test]
    fn rfc9381_example_17() {
        assert_rfc9381_vector(Rfc9381Vector {
            sk: hex32("4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb"),
            pk: hex32("3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c"),
            alpha: b"\x72",
            h: hex32("5b659fc3d4e9263fd9a4ed1d022d75eaacc20df5e09f9ea937502396598dc551"),
            pi: hex80("f3141cd382dc42909d19ec5110469e4feae18300e94f304590abdced48aed5933bf0864a62558b3ed7f2fea45c92a465301b3bbf5e3e54ddf2d935be3b67926da3ef39226bbc355bdc9850112c8f4b02"),
            beta_prefix: hex32("eb4440665d3891d668e7e0fcaf587f1b4bd7fbfe99d0eb2211ccec90496310eb"),
        });
    }

    /// RFC 9381 Appendix B.3 — Example 18.
    #[test]
    fn rfc9381_example_18() {
        assert_rfc9381_vector(Rfc9381Vector {
            sk: hex32("c5aa8df43f9f837bedb7442f31dcb7b166d38535076f094b85ce3a2e0b4458f7"),
            pk: hex32("fc51cd8e6218a1a38da47ed00230f0580816ed13ba3303ac5deb911548908025"),
            alpha: b"\xaf\x82",
            h: hex32("bf4339376f5542811de615e3313d2b36f6f53c0acfebb482159711201192576a"),
            pi: hex80("9bc0f79119cc5604bf02d23b4caede71393cedfbb191434dd016d30177ccbf8096bb474e53895c362d8628ee9f9ea3c0e52c7a5c691b6c18c9979866568add7a2d41b00b05081ed0f58ee5e31b3a970e"),
            beta_prefix: hex32("645427e5d00c62a23fb703732fa5d892940935942101e456ecca7bb217c61c45"),
        });
    }

    struct Rfc9381Vector {
        sk: [u8; 32],
        pk: [u8; 32],
        alpha: &'static [u8],
        h: [u8; 32],
        pi: [u8; 80],
        beta_prefix: [u8; 32],
    }

    fn assert_rfc9381_vector(tv: Rfc9381Vector) {
        let key = VrfKey::from_seed(&tv.sk);
        assert_eq!(key.pubkey(), tv.pk, "public key mismatch");

        let (esk, nonce) = expand_secret_key(&tv.sk);
        assert_eq!(
            encode_to_curve(&tv.pk, tv.alpha)
                .expect("RFC vector hash-to-curve")
                .compress()
                .to_bytes(),
            tv.h,
            "hash-to-curve mismatch"
        );

        let h_bytes = tv.h;
        let k = scalar_from_nonce(&nonce, &h_bytes);
        let h_point = encode_to_curve(&tv.pk, tv.alpha).expect("RFC vector hash-to-curve");
        let gamma = h_point * esk;
        let u = curve25519_dalek::constants::ED25519_BASEPOINT_POINT * k;
        let v = h_point * k;
        let c = hash_points(&tv.pk, &h_bytes, &[gamma, u, v]);
        let s = k + c * esk;
        let built = rfc9381::pack_proof(&gamma, &c, &s);
        assert_eq!(built, tv.pi, "proof mismatch");

        let (proof, beta) = vrf_prove(&key, tv.alpha);
        assert_eq!(proof.0, tv.pi, "vrf_prove mismatch");
        assert_eq!(beta.0, tv.beta_prefix, "beta mismatch");
        vrf_verify(&tv.pk, tv.alpha, &proof).expect("verify must succeed");
    }

    #[test]
    fn tampered_proof_bytes_fail_verify() {
        let key = VrfKey::from_seed(&[11; 32]);
        let (proof, _) = vrf_prove(&key, b"alpha");
        let pk = key.pubkey();

        for idx in [0usize, 31, 32, 47, 48, 79] {
            let mut tampered = proof.0;
            tampered[idx] ^= 0x01;
            assert!(
                vrf_verify(&pk, b"alpha", &VrfProof(tampered)).is_err(),
                "tampered byte at {idx} should fail"
            );
        }
    }

    #[test]
    fn identity_gamma_rejected() {
        let key = VrfKey::from_seed(&[12; 32]);
        let (mut proof, _) = vrf_prove(&key, b"alpha");
        proof.0[..32].copy_from_slice(
            &curve25519_dalek::edwards::EdwardsPoint::identity()
                .compress()
                .to_bytes(),
        );
        assert!(vrf_verify(&key.pubkey(), b"alpha", &proof).is_err());
    }

    #[test]
    fn small_order_gamma_rejected() {
        let key = VrfKey::from_seed(&[13; 32]);
        let (mut proof, _) = vrf_prove(&key, b"alpha");
        // Ed25519 8-torsion point (order 8), not identity.
        proof.0[..32].copy_from_slice(&[
            0xe0, 0xeb, 0x7a, 0x7c, 0x3b, 0x41, 0xb8, 0xae, 0x16, 0x56, 0xe3, 0xfa, 0xf1, 0x9f,
            0xc4, 0x6a, 0x9a, 0x37, 0x90, 0x40, 0x10, 0xf9, 0x4d, 0x39, 0x54, 0x49, 0x69, 0x59,
            0x39, 0x7e, 0xee, 0x55,
        ]);
        assert!(vrf_verify(&key.pubkey(), b"alpha", &proof).is_err());
    }

    #[test]
    fn wrong_pubkey_fails() {
        let k1 = VrfKey::from_seed(&[14; 32]);
        let k2 = VrfKey::from_seed(&[15; 32]);
        let (proof, _) = vrf_prove(&k1, b"alpha");
        assert!(vrf_verify(&k2.pubkey(), b"alpha", &proof).is_err());
    }

    fn hex32(s: &str) -> [u8; 32] {
        hex_bytes(s).try_into().expect("32 bytes")
    }

    fn hex80(s: &str) -> [u8; 80] {
        hex_bytes(s).try_into().expect("80 bytes")
    }

    fn hex_bytes(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }
}
