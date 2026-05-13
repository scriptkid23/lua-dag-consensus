//! Shared data types for LUA-DAG.
//!
//! This crate contains only structs, enums, and codec — no business logic,
//! no signing, no verification. Cryptographic semantics live in `crates/crypto`.
#![cfg_attr(not(test), warn(missing_docs))]

pub mod codec;
pub mod crypto_types;
pub mod dag;
pub mod error;
pub mod macros;
pub mod micro;
pub mod primitives;
pub mod slashing;
pub mod validator;

pub use error::{Error, Result};
