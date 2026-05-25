//! Reed–Solomon erasure coding for L1 blob payloads (07c).

mod config;
mod error;
pub mod commit;
mod gf256;
mod rs;

pub use commit::{rs_merkle_commitment, shard_leaf_hash};
pub use config::ErasureConfig;
pub use error::{ErasureError, Result as ErasureResult};
pub use rs::{decode_shards, encode_shards};
