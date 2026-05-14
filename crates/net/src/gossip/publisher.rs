//! Lightweight publisher that de-dupes recently published payloads
//! using a fixed-size ring buffer of hashes.

use std::collections::VecDeque;

use crypto::hash::{blake3_with_dst, dst};
use types::crypto_types::Hash32;

/// Publisher-side de-dup ring.
#[derive(Debug)]
pub struct Publisher {
    seen: VecDeque<Hash32>,
    capacity: usize,
}

impl Publisher {
    /// New empty publisher with given dedup window.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            seen: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Returns `true` if `payload` was not seen in the recent window and
    /// records it. Returns `false` (drop) if already seen.
    pub fn check(&mut self, payload: &[u8]) -> bool {
        let h = blake3_with_dst(dst::CONTENT_HASH, payload);
        if self.seen.contains(&h) {
            return false;
        }
        if self.seen.len() == self.capacity {
            self.seen.pop_front();
        }
        self.seen.push_back(h);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_within_window_dropped() {
        let mut p = Publisher::new(4);
        assert!(p.check(b"a"));
        assert!(!p.check(b"a"));
        assert!(p.check(b"b"));
    }

    #[test]
    fn evicts_oldest_after_capacity() {
        let mut p = Publisher::new(2);
        p.check(b"a");
        p.check(b"b");
        p.check(b"c"); // evicts "a"
        assert!(p.check(b"a")); // can be added again
    }
}
