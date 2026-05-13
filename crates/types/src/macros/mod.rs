//! L3 macro-finality types.
//!
//! Folder is named `macros` (plural) because `macro` is a Rust keyword.

pub mod checkpoint;
pub mod header;
pub mod proposal;
pub mod qc;

pub use checkpoint::MacroCheckpoint;
pub use header::MacroHeader;
pub use proposal::MacroProposal;
pub use qc::{AggregationMode, MacroQc};
