//! Causal-set / availability DAG inputs consumed by L2.
//!
//! These types are an **input contract**; the L1 implementation (future
//! `crates/dag`) produces them, `consensus` consumes them read-only.

pub mod certified;
pub mod proposal;
pub mod refs;
pub mod vertex;

pub use certified::{CertifiedVertex, SharedCertifiedVertex};
pub use proposal::{VertexPartial, VertexProposal};
pub use refs::{BlobRef, ChunkRef};
pub use vertex::Vertex;
