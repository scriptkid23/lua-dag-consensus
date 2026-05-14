//! Casper-FFG 2-chain finality rule.

use types::crypto_types::Hash32;

/// 2-chain finality tracker.
#[derive(Debug, Default)]
pub struct TwoChainRule {
    /// Hash of the most recently justified checkpoint.
    pub justified_head: Option<Hash32>,
}
