//! L1 causal-set sync RPC types (07c handler lives in `apps/node` JSON-RPC).

use borsh::{BorshDeserialize, BorshSerialize};
use types::{crypto_types::Hash32, primitives::Round};

/// Request: certified vertex hashes for rounds `from..=to`.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct CausalSetReq {
    /// First round.
    pub from: Round,
    /// Last round (inclusive).
    pub to: Round,
}

/// Response: certified vertex hashes in causal order.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct CausalSetResp {
    /// Hashes in causal order.
    pub hashes: Vec<Hash32>,
}
