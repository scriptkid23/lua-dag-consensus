//! Canonical encoding + Blake3-based content hashing.

use borsh::BorshSerialize;

use crate::{
    crypto_types::Hash32,
    error::{Error, Result},
};

/// Serialize `value` to its canonical Borsh byte representation.
pub fn canonical_bytes<T: BorshSerialize>(value: &T) -> Result<Vec<u8>> {
    borsh::to_vec(value).map_err(|e| Error::Codec(e.to_string()))
}

/// Blake3-256 over the canonical bytes of `value`.
///
/// Domain separation is the caller's responsibility — wrap with a tagged
/// struct when reusing this function for different message kinds.
pub fn canonical_hash<T: BorshSerialize>(value: &T) -> Result<Hash32> {
    let bytes = canonical_bytes(value)?;
    let digest = blake3::hash(&bytes);
    Ok(Hash32(*digest.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::Round;

    #[test]
    fn canonical_hash_is_deterministic() {
        let h1 = canonical_hash(&Round(7)).unwrap();
        let h2 = canonical_hash(&Round(7)).unwrap();
        assert_eq!(h1, h2);
    }

    #[test]
    fn canonical_hash_differs_for_different_inputs() {
        let h1 = canonical_hash(&Round(7)).unwrap();
        let h2 = canonical_hash(&Round(8)).unwrap();
        assert_ne!(h1, h2);
    }
}
