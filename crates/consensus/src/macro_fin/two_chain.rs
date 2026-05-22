//! Casper-FFG 2-chain finality rule (L3 03c-1 simplification).

use std::collections::BTreeMap;

use types::{
    crypto_types::Hash32,
    macros::MacroCheckpoint,
    primitives::Height,
};

/// 2-chain finality tracker.
///
/// 03c-1 simplification: justification depth = 1 macro window, finality depth = 2.
/// No epoch source/target arithmetic until 03d wires real surround detection.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TwoChainRule {
    /// All adopted checkpoints, keyed by height (monotonic).
    adopted: BTreeMap<Height, MacroCheckpoint>,
    /// Hash of the most recently justified checkpoint (highest adopted).
    pub justified_head: Option<Hash32>,
    /// Hash of the most recently finalized checkpoint.
    pub finalized_head: Option<Hash32>,
}

impl TwoChainRule {
    /// Mark `cp` as adopted (justified). Idempotent on the same `(height, hash)`.
    pub fn adopt(&mut self, cp: MacroCheckpoint) {
        let entry = self.adopted.entry(cp.height).or_insert_with(|| cp.clone());
        if entry.hash == cp.hash {
            self.justified_head = Some(cp.hash);
        }
    }

    /// Look up the height of the current justified head, if any.
    #[must_use]
    pub fn justified_head_height(&self) -> Option<Height> {
        let head_hash = self.justified_head?;
        self.adopted
            .iter()
            .rev()
            .find(|(_, cp)| cp.hash == head_hash)
            .map(|(h, _)| *h)
    }

    /// True if `height` has been justified (a MacroQc was adopted at it).
    #[must_use]
    pub fn is_justified(&self, height: Height) -> bool {
        self.adopted.contains_key(&height)
    }

    /// If the most recent adoption completes a 2-chain over the previous height,
    /// return the height that is **newly** finalized; else `None`.
    #[must_use]
    pub fn newly_finalized_height(&self) -> Option<Height> {
        let head = self.justified_head_height()?;
        let prev = Height(head.0.checked_sub(1)?);
        let head_cp = self.adopted.get(&head)?;
        let prev_cp = self.adopted.get(&prev)?;
        if head_cp.parent == prev_cp.hash && self.finalized_head != Some(prev_cp.hash) {
            Some(prev)
        } else {
            None
        }
    }

    /// Mark `prev` as finalized (caller invokes after `newly_finalized_height`).
    pub fn mark_finalized(&mut self, prev: Height) {
        if let Some(cp) = self.adopted.get(&prev) {
            self.finalized_head = Some(cp.hash);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::macro_fin::checkpoint::build;
    use types::primitives::Epoch;

    fn ck(height: Height, parent: Hash32, micro: u8) -> MacroCheckpoint {
        build(height, Epoch(0), parent, Hash32([micro; 32]))
    }

    #[test]
    fn genesis_has_no_finality() {
        let r = TwoChainRule::default();
        assert!(r.newly_finalized_height().is_none());
    }

    #[test]
    fn first_adoption_justifies_but_does_not_finalize() {
        let mut r = TwoChainRule::default();
        let c0 = ck(Height(0), Hash32::zero(), 1);
        r.adopt(c0.clone());
        assert_eq!(r.justified_head, Some(c0.hash));
        assert!(r.newly_finalized_height().is_none());
    }

    #[test]
    fn second_adoption_with_matching_parent_finalizes_previous() {
        let mut r = TwoChainRule::default();
        let c0 = ck(Height(0), Hash32::zero(), 1);
        let c1 = ck(Height(1), c0.hash, 2);
        r.adopt(c0.clone());
        r.adopt(c1);
        assert_eq!(r.newly_finalized_height(), Some(Height(0)));
        r.mark_finalized(Height(0));
        assert_eq!(r.finalized_head, Some(c0.hash));
        assert!(r.newly_finalized_height().is_none());
    }

    #[test]
    fn parent_mismatch_blocks_finalization() {
        let mut r = TwoChainRule::default();
        let c0 = ck(Height(0), Hash32::zero(), 1);
        let c1_bad = ck(Height(1), Hash32([0x99; 32]), 2);
        r.adopt(c0);
        r.adopt(c1_bad);
        assert!(r.newly_finalized_height().is_none());
    }
}
