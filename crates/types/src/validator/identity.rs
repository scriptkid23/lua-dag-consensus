//! Validator identity metadata (used for anti-Sybil + diversity scoring).

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// Diversity metadata. All fields are advisory until the anti-Sybil module
/// (out of scope this phase) consumes them.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct ValidatorIdentity {
    /// Autonomous-System number, when known.
    pub asn: Option<u32>,
    /// Cloud-provider tag (e.g. `"aws"`, `"gcp"`, `"hetzner"`).
    pub cloud: Option<String>,
    /// Region code (e.g. `"eu-west-1"`).
    pub region: Option<String>,
}
