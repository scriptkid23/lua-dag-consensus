//! `lock_macro` invariant (whitepaper §13.5).
//!
//! A validator that voted to finalize macro height `h` must not later
//! vote on a conflicting candidate at the same height. This module
//! tracks the per-validator "locked" height; full enforcement happens
//! when `consensus::macro_fin::vote_book` is wired.

use std::collections::HashMap;

use types::{
    crypto_types::Hash32,
    primitives::{Height, ValidatorId},
};

/// Per-validator locks: each validator may pin at most one
/// `(height, checkpoint_hash)` pair as the canonical macro vote.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LockMacro {
    locks: HashMap<ValidatorId, (Height, Hash32)>,
}

impl LockMacro {
    /// New empty tracker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Attempt to lock `validator` to `(height, checkpoint)`. Returns
    /// `Err` if the validator is already locked to a *different*
    /// checkpoint at the same height.
    pub fn try_lock(
        &mut self,
        validator: ValidatorId,
        height: Height,
        checkpoint: Hash32,
    ) -> Result<(), &'static str> {
        match self.locks.get(&validator) {
            Some(&(h, prev)) if h == height && prev != checkpoint => {
                Err("validator already locked to a conflicting checkpoint at this height")
            }
            _ => {
                self.locks.insert(validator, (height, checkpoint));
                Ok(())
            }
        }
    }

    /// Current lock for `validator`, if any.
    #[must_use]
    pub fn get(&self, validator: &ValidatorId) -> Option<(Height, Hash32)> {
        self.locks.get(validator).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_then_extend_to_higher_height_ok() {
        let mut lm = LockMacro::new();
        let v = ValidatorId([1; 32]);
        lm.try_lock(v, Height(1), Hash32([0xAA; 32])).unwrap();
        lm.try_lock(v, Height(2), Hash32([0xBB; 32])).unwrap();
        assert_eq!(lm.get(&v), Some((Height(2), Hash32([0xBB; 32]))));
    }

    #[test]
    fn lock_conflict_at_same_height_rejected() {
        let mut lm = LockMacro::new();
        let v = ValidatorId([1; 32]);
        lm.try_lock(v, Height(1), Hash32([0xAA; 32])).unwrap();
        let err = lm.try_lock(v, Height(1), Hash32([0xCC; 32])).unwrap_err();
        assert!(err.contains("conflicting"));
    }

    #[test]
    fn same_height_same_checkpoint_idempotent() {
        let mut lm = LockMacro::new();
        let v = ValidatorId([1; 32]);
        lm.try_lock(v, Height(1), Hash32([0xAA; 32])).unwrap();
        lm.try_lock(v, Height(1), Hash32([0xAA; 32])).unwrap();
    }
}
