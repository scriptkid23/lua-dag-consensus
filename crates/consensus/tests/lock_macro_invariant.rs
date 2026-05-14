//! Integration check that `LockMacro` rejects conflicting same-height locks.

use consensus::lock_macro::LockMacro;
use types::{
    crypto_types::Hash32,
    primitives::{Height, ValidatorId},
};

#[test]
fn conflicting_lock_rejected_across_validators_independently() {
    let mut lm = LockMacro::new();
    let a = ValidatorId([1; 32]);
    let b = ValidatorId([2; 32]);
    lm.try_lock(a, Height(7), Hash32([0xAA; 32])).unwrap();
    lm.try_lock(b, Height(7), Hash32([0xBB; 32])).unwrap();
    assert!(lm.try_lock(a, Height(7), Hash32([0xCC; 32])).is_err());
    assert!(lm.try_lock(b, Height(7), Hash32([0xBB; 32])).is_ok());
}
