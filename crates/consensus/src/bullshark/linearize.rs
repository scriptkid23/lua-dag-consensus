//! Closure(Aw) BFS linearization, tie-break by vertex hash.

use std::collections::{HashSet, VecDeque};

use crypto::hash::{blake3_with_dst, dst};
use types::{crypto_types::Hash32, dag::CertifiedVertex};

use crate::{error::Result, ports::DagView};

/// Output of linearization: ordered hashes of committed vertices.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Linearization {
    /// Linearized vertex hashes in commit order.
    pub order: Vec<Hash32>,
}

impl Linearization {
    /// BFS over `Closure(anchor)`: walk the anchor's causal past in
    /// breadth-first layers, visiting parents with `hash` tie-break.
    ///
    /// The returned order is **anchor-first**, then each layer of parents
    /// in increasing `Hash32` order. Cycle-safe via a visited set.
    pub fn closure_of_anchor(anchor_hash: Hash32, dag: &dyn DagView) -> Result<Self> {
        let mut visited: HashSet<Hash32> = HashSet::new();
        let mut order: Vec<Hash32> = Vec::new();
        let mut queue: VecDeque<Hash32> = VecDeque::new();
        queue.push_back(anchor_hash);
        visited.insert(anchor_hash);
        while let Some(hash) = queue.pop_front() {
            order.push(hash);
            let Some(cv) = dag.vertex(&hash)? else {
                continue;
            };
            let mut parents = cv.vertex.parents.clone();
            parents.sort_by_key(|h| h.0);
            for p in parents {
                if visited.insert(p) {
                    queue.push_back(p);
                }
            }
        }
        Ok(Self { order })
    }
}

/// Deterministic checkpoint hash over an ordered list of certified vertices.
///
/// The producer hashes only the vertex hashes (not full vertices) under
/// the [`dst::MICRO_QC`] DST so the result matches whatever a verifier
/// re-derives from a `Linearization`.
#[must_use]
pub fn checkpoint_hash_from_rounds(vertices: &[CertifiedVertex]) -> Hash32 {
    let hashes: Vec<Hash32> = vertices.iter().map(|cv| cv.vertex.hash).collect();
    let bytes = borsh::to_vec(&hashes).expect("Hash32 vec is always borsh-serializable");
    blake3_with_dst(dst::MICRO_QC, &bytes)
}

/// Same as [`checkpoint_hash_from_rounds`] but reads the hashes directly
/// from a [`Linearization`] (avoids re-hydrating `CertifiedVertex`).
#[must_use]
pub fn checkpoint_hash_from_linearization(lin: &Linearization) -> Hash32 {
    let bytes = borsh::to_vec(&lin.order).expect("Hash32 vec is always borsh-serializable");
    blake3_with_dst(dst::MICRO_QC, &bytes)
}
