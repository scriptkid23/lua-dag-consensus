//! Helpers that take a 32-byte VRF output and project it onto a uniform
//! `[0, 1)` value for stake-weighted sortition. The actual stake math
//! lives in `consensus::leader::vrf_sortition`.

use types::crypto_types::Hash32;

/// Map a 32-byte VRF output to a fraction in `[0, 1)`.
///
/// Uses the high 53 bits (the f64 mantissa width) as a numerator over
/// `2^53`. This division is exact — `2^53` is representable — so the
/// result is strictly `< 1.0` for every input. Bias is `2^-53`, well
/// below the entropy of any practical committee.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn vrf_to_uniform(beta: &Hash32) -> f64 {
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&beta.0[..8]);
    let n = u64::from_be_bytes(buf) >> 11; // now in [0, 2^53)
    (n as f64) / ((1u64 << 53) as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_maps_to_zero() {
        let u = vrf_to_uniform(&Hash32([0; 32]));
        assert!((u - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn all_ones_maps_close_to_one() {
        let u = vrf_to_uniform(&Hash32([0xFF; 32]));
        assert!(u > 0.999, "got {u}");
        assert!(u < 1.0, "must be < 1");
    }

    #[test]
    fn distinct_outputs_yield_distinct_uniforms() {
        let a = vrf_to_uniform(&Hash32([0xAA; 32]));
        let b = vrf_to_uniform(&Hash32([0xBB; 32]));
        assert!((a - b).abs() > 1e-9);
    }
}
