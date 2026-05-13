//! Deterministic fingerprint over a `DkgCommitment`.

use types::{codec::canonical_hash, crypto_types::Hash32, validator::DkgCommitment};

use crate::error::Result;

/// Deterministic fingerprint suitable for storage indexing.
pub fn commitment_fingerprint(c: &DkgCommitment) -> Result<Hash32> {
    Ok(canonical_hash(c)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::{
        crypto_types::{BlsPubkey, Hash32},
        primitives::{Epoch, ValidatorId},
    };

    #[test]
    fn fingerprint_is_stable() {
        let c = DkgCommitment {
            validator: ValidatorId([1; 32]),
            epoch: Epoch(1),
            bls_pubkey: BlsPubkey([2; 48]),
            shares_root: Hash32([3; 32]),
        };
        let a = commitment_fingerprint(&c).unwrap();
        let b = commitment_fingerprint(&c).unwrap();
        assert_eq!(a, b);
    }
}
