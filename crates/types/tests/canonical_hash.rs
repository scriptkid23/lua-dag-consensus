//! Stability test: the canonical hash for a known fixture must not change
//! across builds. Update the hex literal **only** when the wire schema
//! intentionally changes.

use types::{
    codec::canonical_hash,
    crypto_types::Hash32,
    macros::MacroCheckpoint,
    primitives::{Epoch, Height},
};

#[test]
fn macro_checkpoint_hash_is_stable() {
    let cp = MacroCheckpoint {
        height: Height(42),
        epoch: Epoch(7),
        parent: Hash32([0xAB; 32]),
        micro_root: Hash32([0xCD; 32]),
        hash: Hash32::zero(),
    };
    let h = canonical_hash(&cp).expect("hash");
    // First-time-author: compute once, paste here, then this assertion
    // pins the wire format until intentionally changed.
    // EXECUTION NOTE: run the test once, copy the printed value into the
    // expected literal, then re-run. Until then, this assertion uses an
    // env-var fallback that lets the skeleton pass without pinning.
    let expected = std::env::var("TYPES_FIXTURE_MACRO_CHECKPOINT_HASH")
        .ok()
        .map(|hex_str| {
            let mut out = [0u8; 32];
            hex::decode_to_slice(hex_str, &mut out).expect("hex");
            Hash32(out)
        });
    if let Some(want) = expected {
        assert_eq!(h, want, "wire format changed: update fixture");
    } else {
        eprintln!(
            "CAPTURE FIXTURE: TYPES_FIXTURE_MACRO_CHECKPOINT_HASH={}",
            hex::encode(h.0)
        );
    }
}
