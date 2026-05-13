//! Canonical serialization helpers.
//!
//! Borsh is the on-wire and on-disk codec for LUA-DAG: it has a fixed
//! representation per type and no schema-evolution surprises.

pub mod borsh_impl;

pub use borsh_impl::{canonical_bytes, canonical_hash};
