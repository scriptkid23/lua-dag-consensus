//! libp2p adapter: the only crate that imports `libp2p::*`.
//!
//! All translation between libp2p messages and `consensus::Event` /
//! `consensus::Action` happens in [`bridge`].
#![cfg_attr(not(test), warn(missing_docs))]

pub mod bridge;
pub mod config;
pub mod deterministic_key;
pub mod error;
pub mod gossip;
pub mod gossip_wire;
pub mod identity;
pub mod peers;
pub mod rpc;
pub mod swarm_runner;
pub mod transport;

pub use bridge::{Bridge, BridgeHandle};
pub use config::NetConfig;
pub use deterministic_key::devnet_keypair_from_label;
pub use error::{Error, Result};
pub use transport::{build_transport, build_transport_tcp_only};
