//! Spec §8 — the default `devnet` profile must fail closed if the swarm
//! cannot claim a listen socket and `--allow-skeleton-network` was not
//! passed.
//!
//! Exercises `node::runtime::test_helpers::run_for_test` against the real
//! gate body so a regression that bypasses the check (e.g. moving the gate
//! after swarm init) is caught.

use std::path::PathBuf;

use tempfile::tempdir;

fn write(dir: &std::path::Path, rel: &str, body: &str) -> PathBuf {
    let p = dir.join(rel);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&p, body).unwrap();
    p
}

fn empty_listen_profile() -> &'static str {
    r#"
[node]
network_mode = "live"

[node.identity]
kind = "devnet_seed"
label = "fixture"

[net]
listen = []
bootstrap = []

[net.gossip]
heartbeat_ms = 700
mesh_n = 8
mesh_n_low = 6
mesh_n_high = 12

[net.peers]
max_peers = 64
ban_duration_secs = 600
"#
}

fn write_layered(dir: &std::path::Path) {
    // Base consensus config (Table 17.1).
    let default = consensus::Config::default_table_17_1();
    write(dir, "default.toml", &toml::to_string(&default).unwrap());
    write(dir, "profiles/devnet.toml", empty_listen_profile());
}

#[tokio::test]
async fn live_mode_without_skeleton_flag_refuses_to_start() {
    let dir = tempdir().unwrap();
    write_layered(dir.path());

    let result = node::runtime::test_helpers::run_for_test(node::runtime::test_helpers::TestArgs {
        config_dir: dir.path().to_path_buf(),
        profile: "devnet".into(),
        allow_skeleton_network: false,
    })
    .await;

    let err = result.expect_err("must refuse to start in live mode with empty listen");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("allow-skeleton-network") && msg.contains("06b"),
        "unexpected error: {msg}"
    );
}

#[tokio::test]
async fn allow_skeleton_network_bypasses_the_gate() {
    let dir = tempdir().unwrap();
    write_layered(dir.path());

    let result = node::runtime::test_helpers::run_for_test(node::runtime::test_helpers::TestArgs {
        config_dir: dir.path().to_path_buf(),
        profile: "devnet".into(),
        allow_skeleton_network: true,
    })
    .await;

    // The gate must not be the failure mode here. The helper does not start
    // the orchestrator, so success is `Ok(())`; any other error must NOT
    // mention the listen guard.
    if let Err(err) = result {
        let msg = format!("{err:#}");
        assert!(
            !msg.contains("network_mode=\"live\" requires"),
            "skeleton flag did not bypass the gate: {msg}"
        );
    }
}
