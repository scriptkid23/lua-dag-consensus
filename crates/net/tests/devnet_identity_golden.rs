//! Golden `PeerIDs` for `node0`..`node3` — must match the values used in
//! `docker-compose.yml` `LUA_DAG_BOOTSTRAP_PEERS`.
//!
//! If any of these literals drift, the four-node Compose devnet's bootstrap
//! list points at the wrong `PeerIDs` and nodes will refuse each other. To
//! regenerate after an intentional change (e.g. a new DST), run:
//!
//!     cargo run -p node --bin print_devnet_peer_ids --locked
//!
//! ...and paste the four lines into this constant plus
//! `docker-compose.yml`.

use net::deterministic_key::devnet_keypair_from_label;

const GOLDEN: &[(&str, &str)] = &[
    (
        "node0",
        "12D3KooWNJUN1Vcx4BtroqAX4Eqzoc7AP7k7m4zVPjFHgqU4rgSL",
    ),
    (
        "node1",
        "12D3KooWL1XMKGUfDJc5ynvUjQ7tXsFkbcn5drxseJCczacaXTJN",
    ),
    (
        "node2",
        "12D3KooWSPCsqrqrQY5yhXR2xkbNt1aSaYBe2EAK42PQjEBcnjtc",
    ),
    (
        "node3",
        "12D3KooWHEgZEY1X7fAHjb9GgrdNQFrVvs7dAh7CcWVqpxeqEiMq",
    ),
];

#[test]
fn golden_peer_ids_are_stable() {
    for (label, expected) in GOLDEN {
        let kp = devnet_keypair_from_label(label).unwrap();
        assert_eq!(
            kp.public().to_peer_id().to_string(),
            *expected,
            "PeerID for label `{label}` drifted — regenerate compose bootstrap with `cargo run -p node --bin print_devnet_peer_ids`"
        );
    }
}
