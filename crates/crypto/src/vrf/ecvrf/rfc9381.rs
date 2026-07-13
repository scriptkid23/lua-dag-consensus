//! Internal RFC 9381 ECVRF-EDWARDS25519-SHA512-TAI (Section 5.5).

use curve25519_dalek::{
    constants::{ED25519_BASEPOINT_POINT, ED25519_BASEPOINT_TABLE},
    edwards::{CompressedEdwardsY, EdwardsPoint},
    scalar::Scalar,
    traits::IsIdentity,
};
use sha2::{Digest, Sha512};

/// Ciphersuite octet (RFC 9381 §8.5).
const SUITE: u8 = 0x03;
const ZERO: u8 = 0x00;
const ONE: u8 = 0x01;
const TWO: u8 = 0x02;
const THREE: u8 = 0x03;

/// Expand a 32-byte Ed25519 seed per RFC 8032 §5.1.5.
pub(super) fn expand_secret_key(seed: &[u8; 32]) -> (Scalar, [u8; 32]) {
    let hash: [u8; 64] = Sha512::digest(seed.as_slice()).into();
    let mut lower = [0u8; 32];
    let mut upper = [0u8; 32];
    lower.copy_from_slice(&hash[..32]);
    upper.copy_from_slice(&hash[32..]);
    lower[0] &= 248;
    lower[31] &= 63;
    lower[31] |= 64;
    (Scalar::from_bytes_mod_order(lower), upper)
}

/// RFC 9381 §5.4.1.1 — try-and-increment hash-to-curve (ctr bounded to 0..=255).
pub(super) fn encode_to_curve(pk_bytes: &[u8; 32], alpha: &[u8]) -> Option<EdwardsPoint> {
    for counter in 0u8..=255 {
        let hash: [u8; 64] = Sha512::new()
            .chain_update([SUITE, ONE])
            .chain_update(pk_bytes)
            .chain_update(alpha)
            .chain_update([counter, ZERO])
            .finalize()
            .into();
        let mut hash32 = [0u8; 32];
        hash32.copy_from_slice(&hash[..32]);
        if let Some(point) = interpret_hash_value_as_a_point(hash32) {
            let result = point.mul_by_cofactor();
            if is_valid_curve_point(&result) {
                return Some(result);
            }
        }
    }
    None
}

/// Reject identity and small-order points (prime-subgroup only).
fn is_valid_curve_point(point: &EdwardsPoint) -> bool {
    !point.is_identity() && !point.is_small_order()
}

/// RFC 8032 §5.1.3 — reject invalid encodings before decompress.
fn interpret_hash_value_as_a_point(hash: [u8; 32]) -> Option<EdwardsPoint> {
    let is_invalid = hash[1..=30].iter().all(|b| *b == 255)
        && (hash[31] == 255 || hash[31] == 127)
        && [1u8, 3, 4, 5, 9, 10, 13, 14, 15, 16].contains(&((256u16 - hash[0] as u16) as u8));
    if is_invalid {
        return None;
    }
    CompressedEdwardsY::from_slice(&hash)
        .ok()?
        .decompress()
}

pub(super) fn scalar_from_nonce(nonce: &[u8; 32], h_point_bytes: &[u8; 32]) -> Scalar {
    let k_wide: [u8; 64] = Sha512::new()
        .chain_update(nonce)
        .chain_update(h_point_bytes)
        .finalize()
        .into();
    Scalar::from_bytes_mod_order_wide(&k_wide)
}

pub(super) fn hash_points(
    pk_bytes: &[u8; 32],
    h_point_bytes: &[u8; 32],
    points: &[EdwardsPoint],
) -> Scalar {
    let mut hasher = Sha512::new();
    hasher.update([SUITE, TWO]);
    hasher.update(pk_bytes);
    hasher.update(h_point_bytes);
    for point in points {
        hasher.update(point.compress().to_bytes());
    }
    hasher.update([ZERO]);
    let hash: [u8; 64] = hasher.finalize().into();
    let mut c_buf = [0u8; 32];
    c_buf[..16].copy_from_slice(&hash[..16]);
    Scalar::from_bytes_mod_order(c_buf)
}

/// RFC 9381 §5.2 — first 32 bytes of SHA-512 output used as wire `Hash32`.
pub(super) fn gamma_to_hash32(gamma: &EdwardsPoint) -> [u8; 32] {
    let full: [u8; 64] = Sha512::new()
        .chain_update([SUITE, THREE])
        .chain_update(gamma.mul_by_cofactor().compress().as_bytes())
        .chain_update([ZERO])
        .finalize()
        .into();
    let mut beta = [0u8; 32];
    beta.copy_from_slice(&full[..32]);
    beta
}

pub(super) struct ParsedProof {
    pub gamma: EdwardsPoint,
    pub c: Scalar,
    pub s: Scalar,
}

pub(super) fn parse_proof(bytes: &[u8; 80]) -> Option<ParsedProof> {
    let gamma = CompressedEdwardsY::from_slice(&bytes[..32])
        .ok()?
        .decompress()?;
    if !is_valid_curve_point(&gamma) {
        return None;
    }
    let mut c_buf = [0u8; 32];
    c_buf[..16].copy_from_slice(&bytes[32..48]);
    let s_bytes: [u8; 32] = bytes[48..80].try_into().ok()?;
    let s = Scalar::from_canonical_bytes(s_bytes).into_option()?;
    Some(ParsedProof {
        gamma,
        c: Scalar::from_bytes_mod_order(c_buf),
        s,
    })
}

pub(super) fn pack_proof(gamma: &EdwardsPoint, c: &Scalar, s: &Scalar) -> [u8; 80] {
    let mut proof = [0u8; 80];
    proof[..32].copy_from_slice(&gamma.compress().to_bytes());
    proof[32..48].copy_from_slice(&c.to_bytes()[..16]);
    proof[48..].copy_from_slice(&s.to_bytes());
    proof
}

pub(super) fn prove_internal(
    sk: &Scalar,
    nonce: &[u8; 32],
    pk_bytes: &[u8; 32],
    alpha: &[u8],
) -> [u8; 80] {
    let h_point = encode_to_curve(pk_bytes, alpha)
        .expect("hash-to-curve failed after 256 tries (RFC 9381 §5.4.1.1)");
    let h_bytes = h_point.compress().to_bytes();
    let k = scalar_from_nonce(nonce, &h_bytes);
    let gamma = h_point * sk;
    let u = ED25519_BASEPOINT_TABLE * &k;
    let v = h_point * k;
    let c = hash_points(pk_bytes, &h_bytes, &[gamma, u, v]);
    let s = k + c * sk;
    pack_proof(&gamma, &c, &s)
}

pub(super) fn verify_internal(
    pk_point: &EdwardsPoint,
    pk_bytes: &[u8; 32],
    alpha: &[u8],
    proof: &ParsedProof,
) -> bool {
    let Some(h_point) = encode_to_curve(pk_bytes, alpha) else {
        return false;
    };
    let h_bytes = h_point.compress().to_bytes();
    let u = ED25519_BASEPOINT_POINT * proof.s - pk_point * proof.c;
    let v = h_point * proof.s - proof.gamma * proof.c;
    let c_prime = hash_points(pk_bytes, &h_bytes, &[proof.gamma, u, v]);
    proof.c == c_prime
}

pub(super) fn decompress_pubkey(pk_bytes: &[u8; 32]) -> Option<EdwardsPoint> {
    let point = CompressedEdwardsY::from_slice(pk_bytes)
        .ok()?
        .decompress()?;
    if point.is_small_order() {
        return None;
    }
    Some(point)
}
