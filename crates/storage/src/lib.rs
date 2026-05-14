//! `RocksDB`-backed persistence layer.
//!
//! Public surface is the [`RocksPersistence`] struct, which implements
//! [`consensus::ports::Persistence`].
#![cfg_attr(not(test), warn(missing_docs))]

pub mod columns;
pub mod config;
pub mod db;
pub mod error;
pub mod gc;
pub mod keys;
pub mod persistence_impl;
pub mod snapshot;
pub mod stores;
pub mod wal;

pub use config::StorageConfig;
pub use db::Database;
pub use error::{Error, Result};
pub use persistence_impl::RocksPersistence;
