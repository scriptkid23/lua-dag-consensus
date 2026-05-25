use crypto::hash::{blake3_with_dst, dst};
use types::crypto_types::Hash32;

/// Leaf hash for one erasure shard.
#[must_use]
pub fn shard_leaf_hash(shard: &[u8]) -> Hash32 {
    blake3_with_dst(dst::BLOB_SHARD, shard)
}

/// RS merkle commitment carried in [`types::dag::BlobRef`] when erasure is enabled.
#[must_use]
pub fn rs_merkle_commitment(shards: &[Vec<u8>]) -> Hash32 {
    let leaves: Vec<Hash32> = shards.iter().map(|s| shard_leaf_hash(s)).collect();
    let root = binary_merkle_root(&leaves);
    blake3_with_dst(dst::BLOB_RS_ROOT, root.as_bytes())
}

fn binary_merkle_root(leaves: &[Hash32]) -> Hash32 {
    assert!(!leaves.is_empty(), "merkle tree needs leaves");
    let mut layer = leaves.to_vec();
    while layer.len() > 1 {
        let mut next = Vec::with_capacity(layer.len().div_ceil(2));
        for pair in layer.chunks(2) {
            let mut buf = [0u8; 64];
            buf[..32].copy_from_slice(pair[0].as_bytes());
            if pair.len() == 2 {
                buf[32..].copy_from_slice(pair[1].as_bytes());
            } else {
                buf[32..].copy_from_slice(pair[0].as_bytes());
            }
            next.push(blake3_with_dst(dst::BLOB_MERKLE_NODE, &buf));
        }
        layer = next;
    }
    layer[0]
}
