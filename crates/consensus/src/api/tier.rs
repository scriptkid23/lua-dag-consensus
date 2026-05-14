//! Blob lifecycle (Appendix A):
//! `accepted → soft_confirmed → justified → finalized → epoch_finalized`.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// Externally-visible status of a blob.
#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum BlobStatus {
    /// L1 accepted (custody acknowledged).
    Accepted = 0,
    /// L2 micro-committed (within wave).
    SoftConfirmed = 1,
    /// L3 justified (one macro window of 2-chain).
    Justified = 2,
    /// L3 finalized (full 2-chain).
    Finalized = 3,
    /// L4 anchored to Bitcoin (placeholder for future).
    EpochFinalized = 4,
}
