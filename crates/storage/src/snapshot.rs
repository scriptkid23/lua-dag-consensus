//! State snapshots for late-joining validators + WS bootstrap.
//!
//! Skeleton: exposes a deterministic snapshot identifier helper. Real
//! snapshot creation (`RocksDB` SST export) lands in a follow-up.

use types::{crypto_types::Hash32, primitives::Height};

use crypto::hash::{blake3_with_dst, dst};

/// Compute a deterministic snapshot identifier from `(height, root)`.
#[must_use]
pub fn snapshot_id(height: Height, macro_root: &Hash32) -> Hash32 {
    let mut buf = [0u8; 40];
    buf[..8].copy_from_slice(&height.0.to_be_bytes());
    buf[8..].copy_from_slice(&macro_root.0);
    blake3_with_dst(dst::CONTENT_HASH, &buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_id_is_deterministic() {
        let h = Height(42);
        let r = Hash32([7; 32]);
        assert_eq!(snapshot_id(h, &r), snapshot_id(h, &r));
    }
}
