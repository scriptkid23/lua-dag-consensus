//! libp2p adapter: the only crate that imports `libp2p::*`.
//!
//! All translation between libp2p messages and `consensus::Event` /
//! `consensus::Action` happens in [`bridge`].
#![cfg_attr(not(test), warn(missing_docs))]

pub mod bridge;
pub mod config;
pub mod error;
pub mod gossip;
pub mod identity;
pub mod peers;
pub mod rpc;
pub mod transport;

pub use bridge::{Bridge, BridgeHandle};
pub use config::NetConfig;
pub use error::{Error, Result};
