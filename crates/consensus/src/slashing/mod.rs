//! Slashing detectors + penalty math.

pub mod double_vote;
pub mod equivocation;
pub mod evidence;
pub mod inactivity_leak;
pub mod penalty;
pub mod surround;

pub use evidence::verify_evidence;
pub use penalty::Penalty;
