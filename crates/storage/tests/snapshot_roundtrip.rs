//! Snapshot-id determinism.

use storage::snapshot::snapshot_id;
use types::{crypto_types::Hash32, primitives::Height};

#[test]
fn id_is_deterministic_and_distinct_per_input() {
    let a = snapshot_id(Height(1), &Hash32([1; 32]));
    let b = snapshot_id(Height(1), &Hash32([1; 32]));
    let c = snapshot_id(Height(2), &Hash32([1; 32]));
    assert_eq!(a, b);
    assert_ne!(a, c);
}
