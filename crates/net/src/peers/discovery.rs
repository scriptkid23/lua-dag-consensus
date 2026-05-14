//! Bootstrap + Kademlia discovery (optional).

use serde::{Deserialize, Serialize};

/// Discovery configuration; populated from `NetConfig.bootstrap` at
/// startup.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    /// Whether to enable Kademlia DHT alongside the static bootstrap list.
    pub enable_kad: bool,
}
