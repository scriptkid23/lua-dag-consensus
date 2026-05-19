//! Layered node configuration (spec §3.1 / §3.4).
//!
//! Layers (last wins):
//!   1. `config/default.toml`        — consensus tables (whitepaper Table 17.1)
//!   2. `config/profiles/<profile>`  — `[node]` / `[net]` / `[rocksdb]`
//!   3. `config/local.toml`          — optional, gitignored
//!   4. `--override-config <path>`   — repeatable
//!   5. env whitelist (`LUA_DAG_*`)  — replaces specific fields post-merge
//!   6. CLI flags                    — explicit `--data-dir`, listen addrs, etc.

use anyhow::{Context, Result};

use crate::args::Args;
use crate::config_layers::{self, NodeSection, ProfileFile};

/// Combined config wrapper.
#[derive(Clone, Debug)]
pub struct NodeConfig {
    /// Protocol parameters (whitepaper Table 17.1).
    pub consensus: consensus::Config,
    /// `[node]` section.
    pub node: NodeSection,
    /// Network parameters.
    pub net: net::NetConfig,
    /// Storage parameters.
    pub storage: storage::StorageConfig,
    /// Admin HTTP bind (prometheus + health).
    pub admin_listen: String,
    /// JSON-RPC HTTP bind.
    pub rpc_listen: String,
}

impl NodeConfig {
    /// Build the merged config from CLI args.
    pub fn load(args: &Args) -> Result<Self> {
        // Merge all TOML layers once; we extract the consensus and profile shapes
        // from the same `toml::Value` tree so a single `local.toml` can override
        // either.
        let merged =
            config_layers::merge_layers(&args.config_dir, &args.profile, &args.override_configs)
                .context("merge layered TOML config")?;

        // Extract the profile-side shape (`[node]`, `[net]`, `[rocksdb]`).
        let mut profile: ProfileFile = merged
            .clone()
            .try_into()
            .context("merged config does not match ProfileFile schema")?;

        // Extract consensus config (whitepaper Table 17.1) from the same merged tree.
        let consensus_cfg = consensus_config_from_value(merged)
            .context("parse consensus section from merged config")?;

        // ──── env whitelist (applied after file merge, before CLI flags) ────

        // LUA_DAG_NODE_IDENTITY_LABEL overrides `[node.identity].label`.
        if let Some(label) = args.identity_label.as_deref() {
            profile.node.identity.label = label.to_string();
        }

        // LUA_DAG_BOOTSTRAP_PEERS replaces the merged bootstrap list wholesale
        // (spec §3.2 recommendation (a)). Empty string is treated as unset.
        if let Ok(raw) = std::env::var("LUA_DAG_BOOTSTRAP_PEERS") {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                profile.net.bootstrap = trimmed
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect();
            }
        }

        // ──── identity-kind validation (spec §3.4) ────
        match (
            profile.node.identity.kind.as_str(),
            args.allow_skeleton_network,
        ) {
            ("devnet_seed", _) => {}
            (other, true) => tracing::warn!(
                kind = other,
                "unknown identity kind; allowed by --allow-skeleton-network"
            ),
            (other, false) => anyhow::bail!(
                "unknown node.identity.kind `{other}`; only `devnet_seed` is supported for live mode"
            ),
        }

        // ──── storage path resolution: profile [rocksdb] → CLI override → default ────
        let mut storage_cfg = profile
            .rocksdb
            .clone()
            .unwrap_or_else(storage::StorageConfig::devnet_default);
        if let Some(d) = &args.data_dir {
            storage_cfg.path.clone_from(d);
        }

        Ok(Self {
            consensus: consensus_cfg,
            node: profile.node,
            net: profile.net,
            storage: storage_cfg,
            admin_listen: args.admin_listen.clone(),
            rpc_listen: args.rpc_listen.clone(),
        })
    }
}

/// Extract `consensus::Config` from a merged TOML `Value` tree.
///
/// Re-serializes the relevant tables to a TOML string and feeds that to the
/// canonical `Config::from_toml_str` so the `schema_version` check is preserved.
fn consensus_config_from_value(merged: toml::Value) -> Result<consensus::Config> {
    // `consensus::Config` expects a flat top-level document with `schema_version`,
    // `timing`, `bullshark`, etc. The merged tree already has those at the root
    // (default.toml contributes them), so we serialize a filtered copy back to a
    // TOML string and hand it to `from_toml_str`.
    let toml::Value::Table(mut table) = merged else {
        anyhow::bail!("merged config must be a table");
    };
    // Strip profile-only keys; everything else (timing, bullshark, ...) is
    // consensus's domain. Unknown extras would be ignored by serde anyway, but
    // serializing back and forth is cheaper without them.
    for key in ["node", "net", "rocksdb"] {
        table.remove(key);
    }
    let restored = toml::Value::Table(table);
    let s = toml::to_string(&restored).context("re-serialize consensus subset")?;
    consensus::Config::from_toml_str(&s).map_err(anyhow::Error::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_layered_fixture(dir: &std::path::Path) {
        // base default.toml: consensus tables (whitepaper Table 17.1).
        let default = consensus::Config::default_table_17_1();
        let mut f = std::fs::File::create(dir.join("default.toml")).unwrap();
        f.write_all(toml::to_string(&default).unwrap().as_bytes())
            .unwrap();

        // profile devnet.toml: [node]/[net]/[rocksdb].
        std::fs::create_dir_all(dir.join("profiles")).unwrap();
        let mut f = std::fs::File::create(dir.join("profiles/devnet.toml")).unwrap();
        f.write_all(
            br#"
[node]
network_mode = "live"

[node.identity]
kind = "devnet_seed"
label = "node0"

[net]
listen = ["/ip4/0.0.0.0/tcp/9000"]
bootstrap = ["/dns4/x/tcp/9000"]

[net.gossip]
heartbeat_ms = 700
mesh_n = 8
mesh_n_low = 6
mesh_n_high = 12

[net.peers]
max_peers = 64
ban_duration_secs = 600
"#,
        )
        .unwrap();
    }

    fn build_args(config_dir: std::path::PathBuf, identity_label: Option<String>) -> Args {
        Args {
            profile: "devnet".into(),
            config_dir,
            override_configs: vec![],
            identity_label,
            allow_skeleton_network: false,
            data_dir: None,
            admin_listen: "127.0.0.1:0".into(),
            rpc_listen: "127.0.0.1:0".into(),
            health_probe: false,
        }
    }

    #[test]
    fn load_with_default_plus_profile() {
        let dir = tempfile::tempdir().unwrap();
        write_layered_fixture(dir.path());
        let args = build_args(dir.path().to_path_buf(), None);
        let cfg = NodeConfig::load(&args).unwrap();
        assert_eq!(cfg.node.identity.label, "node0");
        assert_eq!(cfg.consensus.schema_version, 1);
        assert_eq!(cfg.net.listen.len(), 1);
        assert_eq!(cfg.net.bootstrap.len(), 1);
    }

    #[test]
    fn identity_label_arg_overrides_profile() {
        let dir = tempfile::tempdir().unwrap();
        write_layered_fixture(dir.path());
        let args = build_args(dir.path().to_path_buf(), Some("node2".into()));
        let cfg = NodeConfig::load(&args).unwrap();
        assert_eq!(cfg.node.identity.label, "node2");
    }
}
