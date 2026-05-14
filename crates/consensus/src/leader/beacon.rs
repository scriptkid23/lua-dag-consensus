//! Beacon chaining: `R_w = H(R_{w-1} || MacroQC)` (Eq. 8.1).

use crypto::hash::{blake3_with_dst, dst};
use types::crypto_types::Hash32;

/// Compute the next beacon output from the previous beacon and a macro QC hash.
#[must_use]
pub fn chain_beacon(prev: &Hash32, macro_qc_hash: &Hash32) -> Hash32 {
    let mut buf = [0u8; 64];
    buf[..32].copy_from_slice(&prev.0);
    buf[32..].copy_from_slice(&macro_qc_hash.0);
    blake3_with_dst(dst::BEACON, &buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chaining_is_deterministic_and_changes_on_input() {
        let prev = Hash32([1; 32]);
        let qc = Hash32([2; 32]);
        let a = chain_beacon(&prev, &qc);
        let b = chain_beacon(&prev, &qc);
        assert_eq!(a, b);
        let c = chain_beacon(&prev, &Hash32([3; 32]));
        assert_ne!(a, c);
    }
}
