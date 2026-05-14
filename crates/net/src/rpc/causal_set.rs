//! L1 causal-set sync RPC (placeholder until `crates/dag` lands).

use borsh::{BorshDeserialize, BorshSerialize};
use types::{crypto_types::Hash32, primitives::Round};

/// Request: "give me certified vertices for rounds `from..=to`".
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct CausalSetReq {
    /// First round.
    pub from: Round,
    /// Last round (inclusive).
    pub to: Round,
}

/// Response: list of certified vertex hashes (full bodies fetched via
/// `crates/dag` in a follow-up).
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct CausalSetResp {
    /// Hashes in causal order.
    pub hashes: Vec<Hash32>,
}
