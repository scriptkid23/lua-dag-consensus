//! DKG scaffolding.
//!
//! Full DKG ceremony is out of scope this phase (spec §2.2). This module
//! ships only a fingerprint helper used by storage/observability.

pub mod fingerprint;

pub use fingerprint::commitment_fingerprint;
