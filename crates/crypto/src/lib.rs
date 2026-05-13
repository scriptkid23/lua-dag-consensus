//! Cryptographic primitives for LUA-DAG.
//!
//! This crate wraps `blst` (BLS12-381) and `curve25519-dalek` (ECVRF on
//! Edwards25519) behind a small surface re-using the wire types from
//! `crates/types`. Aggregation, sortition, KDF, and a DKG fingerprint
//! stub live here as well.
#![cfg_attr(not(test), warn(missing_docs))]
// `blst` requires `unsafe` FFI calls. Override the workspace
// `unsafe_code = "forbid"` lint for this crate only.
#![allow(unsafe_code)]

pub mod bls;
pub mod dkg;
pub mod error;
pub mod hash;
pub mod kdf;
pub mod vrf;

pub use error::{Error, Result};
