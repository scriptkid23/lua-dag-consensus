//! Canonical vertex signing root and content hash.

use borsh::BorshSerialize;
use crypto::hash::{blake3_with_dst, dst};
use types::{
    crypto_types::Hash32,
    dag::{BlobRef, Vertex},
    primitives::{Round, ValidatorId},
};

/// Vertex body for signing: same fields as [`Vertex`] but hash pinned to zero.
#[derive(BorshSerialize)]
struct SignableVertex<'a> {
    round: Round,
    author: ValidatorId,
    parents: &'a [Hash32],
    blobs: &'a [BlobRef],
    hash: Hash32,
}

/// Canonical signing bytes for a vertex (excludes real content hash).
pub fn signing_bytes(vertex: &Vertex) -> Vec<u8> {
    let signable = SignableVertex {
        round: vertex.round,
        author: vertex.author,
        parents: &vertex.parents,
        blobs: &vertex.blobs,
        hash: Hash32([0u8; 32]),
    };
    borsh::to_vec(&signable).expect("vertex signing root must borsh")
}

/// Production content hash for a vertex body.
#[must_use]
pub fn content_hash(vertex: &Vertex) -> Hash32 {
    blake3_with_dst(dst::VERTEX_HASH, &signing_bytes(vertex))
}

/// Attach the content hash to an uncertified vertex (mutates hash field).
pub fn seal_hash(vertex: &mut Vertex) {
    vertex.hash = content_hash(vertex);
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::primitives::Round;

    #[test]
    fn content_hash_is_deterministic() {
        let mut v = Vertex {
            round: Round(3),
            author: ValidatorId([1u8; 32]),
            parents: vec![Hash32([2u8; 32])],
            blobs: vec![],
            hash: Hash32([0u8; 32]),
        };
        let h1 = content_hash(&v);
        let h2 = content_hash(&v);
        assert_eq!(h1, h2);
        seal_hash(&mut v);
        assert_eq!(v.hash, h1);
    }

    #[test]
    fn changing_parents_changes_hash() {
        let base = Vertex {
            round: Round(1),
            author: ValidatorId([0u8; 32]),
            parents: vec![],
            blobs: vec![],
            hash: Hash32([0u8; 32]),
        };
        let mut with_parent = base.clone();
        with_parent.parents.push(Hash32([9u8; 32]));
        assert_ne!(content_hash(&base), content_hash(&with_parent));
    }
}
