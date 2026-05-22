//! Build canonical `MacroCheckpoint`s.

use borsh::BorshSerialize;
use crypto::hash::{blake3_with_dst, dst};
use types::{
    crypto_types::Hash32,
    macros::MacroCheckpoint,
    primitives::{Epoch, Height},
};

/// **Deprecated skeleton** — kept so existing re-exports continue to compile.
/// Real construction goes through [`build`].
#[derive(Debug, Default)]
pub struct CheckpointBuilder;

impl CheckpointBuilder {
    /// Skeleton retained for backward-compat; new code uses [`build`].
    #[must_use]
    pub fn placeholder() -> Self {
        Self
    }
}

/// Build a `MacroCheckpoint` with a canonical `hash` field.
///
/// Hash is computed over the borsh encoding of the unhashed fields
/// (`height || epoch || parent || micro_root`) prefixed with
/// [`dst::MACRO_CHECKPOINT`]. Two callers feeding identical inputs
/// produce identical `MacroCheckpoint.hash`.
#[must_use]
pub fn build(height: Height, epoch: Epoch, parent: Hash32, micro_root: Hash32) -> MacroCheckpoint {
    #[derive(BorshSerialize)]
    struct Preimage {
        height: u64,
        epoch: u64,
        parent: [u8; 32],
        micro_root: [u8; 32],
    }
    let pre = Preimage {
        height: height.0,
        epoch: epoch.0,
        parent: parent.0,
        micro_root: micro_root.0,
    };
    let bytes = borsh::to_vec(&pre).expect("borsh encode preimage");
    let hash = blake3_with_dst(dst::MACRO_CHECKPOINT, &bytes);
    MacroCheckpoint {
        height,
        epoch,
        parent,
        micro_root,
        hash,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_is_deterministic_for_same_inputs() {
        let a = build(Height(7), Epoch(0), Hash32([0xAA; 32]), Hash32([0xBB; 32]));
        let b = build(Height(7), Epoch(0), Hash32([0xAA; 32]), Hash32([0xBB; 32]));
        assert_eq!(a, b);
        assert_eq!(a.hash, b.hash);
    }

    #[test]
    fn build_hash_changes_with_each_field() {
        let base = build(Height(7), Epoch(0), Hash32([0xAA; 32]), Hash32([0xBB; 32]));
        assert_ne!(
            base.hash,
            build(Height(8), Epoch(0), Hash32([0xAA; 32]), Hash32([0xBB; 32])).hash
        );
        assert_ne!(
            base.hash,
            build(Height(7), Epoch(1), Hash32([0xAA; 32]), Hash32([0xBB; 32])).hash
        );
        assert_ne!(
            base.hash,
            build(Height(7), Epoch(0), Hash32([0xCC; 32]), Hash32([0xBB; 32])).hash
        );
        assert_ne!(
            base.hash,
            build(Height(7), Epoch(0), Hash32([0xAA; 32]), Hash32([0xDD; 32])).hash
        );
    }
}
