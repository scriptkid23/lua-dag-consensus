//! Prints the deterministic devnet `PeerIDs` for labels `node0`..`node3`.
//!
//! Use the output to populate `LUA_DAG_BOOTSTRAP_PEERS` in `docker-compose.yml`
//! and the golden literals in `crates/net/tests/devnet_identity_golden.rs`.
//!
//! The values are derived from the BLAKE3 DST + label and are stable across
//! machines — once committed they only change if the DST or label changes.

use net::deterministic_key::devnet_keypair_from_label;

fn main() {
    for label in ["node0", "node1", "node2", "node3"] {
        let kp = devnet_keypair_from_label(label).expect("derive key");
        println!("{label} {}", kp.public().to_peer_id());
    }
}
