//! Spec §8 — live mode gate respects `l3_wire_complete` (plan 06b-l3).

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

fn live_profile(l3_wire_complete: bool) -> String {
    format!(
        r#"
[node]
network_mode = "live"
l3_wire_complete = {l3_wire_complete}

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
    )
}

fn write_layered(dir: &std::path::Path, l3_wire_complete: bool) {
    let default = consensus::Config::default_table_17_1();
    write(dir, "default.toml", &toml::to_string(&default).unwrap());
    write(
        dir,
        "profiles/devnet.toml",
        &live_profile(l3_wire_complete),
    );
}

#[tokio::test]
async fn live_mode_without_l3_wire_complete_refuses_to_start() {
    let dir = tempdir().unwrap();
    write_layered(dir.path(), false);

    let result = node::runtime::test_helpers::run_for_test(node::runtime::test_helpers::TestArgs {
        config_dir: dir.path().to_path_buf(),
        profile: "devnet".into(),
        allow_skeleton_network: false,
    })
    .await;

    let err = result.expect_err("must refuse when l3_wire_complete=false");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("l3_wire_complete") || msg.contains("allow-skeleton-network"),
        "unexpected error: {msg}"
    );
}

#[tokio::test]
async fn live_mode_passes_gate_when_l3_wire_complete() {
    let dir = tempdir().unwrap();
    write_layered(dir.path(), true);

    let result = node::runtime::test_helpers::run_for_test(node::runtime::test_helpers::TestArgs {
        config_dir: dir.path().to_path_buf(),
        profile: "devnet".into(),
        allow_skeleton_network: false,
    })
    .await;

    result.expect("live mode should pass gate when l3_wire_complete=true");
}

#[tokio::test]
async fn allow_skeleton_network_bypasses_the_gate() {
    let dir = tempdir().unwrap();
    write_layered(dir.path(), false);

    let result = node::runtime::test_helpers::run_for_test(node::runtime::test_helpers::TestArgs {
        config_dir: dir.path().to_path_buf(),
        profile: "devnet".into(),
        allow_skeleton_network: true,
    })
    .await;

    if let Err(err) = result {
        let msg = format!("{err:#}");
        assert!(
            !msg.contains("network_mode=\"live\" requires"),
            "skeleton flag did not bypass the gate: {msg}"
        );
    }
}
