//! Bullshark micro-ordering (whitepaper §8).
//!
//! Implementation lands in follow-up plans:
//!   * Wave structure + anchor selection → plan 03b.
//!   * Commit rule (shortcut + slow path)  → plan 03b.
//!   * Linearization (BFS over Closure(Aw)) → plan 03b.
//!   * MicroQc aggregation                  → plan 03b.

pub mod anchor;
pub mod commit;
pub mod linearize;
pub mod micro_qc;
pub mod wave;

pub use anchor::AnchorChoice;
pub use commit::{CommitDecision, CommitPath};
pub use linearize::Linearization;
pub use micro_qc::MicroQcBuilder;
pub use wave::WaveId;
