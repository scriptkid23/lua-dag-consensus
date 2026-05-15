//! Layered node configuration:
//! 1. `config/default.toml` (mandatory, mirrors Table 17.1).
//! 2. Optional `--override-config` TOML (last-write-wins).
//! 3. CLI flag overrides (`--data-dir`, `--admin-listen`, …).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::args::Args;

/// Combined config wrapper.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Protocol parameters (whitepaper Table 17.1).
    pub consensus: consensus::Config,
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
        let raw = std::fs::read_to_string(&args.default_config).with_context(|| {
            format!("read default config from {}", args.default_config.display())
        })?;
        let consensus_cfg = consensus::Config::from_toml_str(&raw)
            .map_err(anyhow::Error::from)
            .context("parse consensus section of default.toml")?;

        // Optional override TOML — only parses the consensus section if
        // provided; net + storage default to devnet at this skeleton stage.
        let consensus_cfg = if let Some(path) = &args.override_config {
            let raw = std::fs::read_to_string(path)
                .with_context(|| format!("read override config from {}", path.display()))?;
            consensus::Config::from_toml_str(&raw)
                .map_err(anyhow::Error::from)
                .unwrap_or(consensus_cfg)
        } else {
            consensus_cfg
        };

        let mut storage_cfg = storage::StorageConfig::devnet_default();
        if let Some(d) = &args.data_dir {
            storage_cfg.path.clone_from(d);
        }

        Ok(Self {
            consensus: consensus_cfg,
            net: net::NetConfig::devnet_default(),
            storage: storage_cfg,
            admin_listen: args.admin_listen.clone(),
            rpc_listen: args.rpc_listen.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use crate::args::Args;

    #[test]
    fn load_with_default_config_only() {
        let dir = tempfile::tempdir().unwrap();
        let cfg_path = dir.path().join("default.toml");
        let mut f = std::fs::File::create(&cfg_path).unwrap();
        let default = consensus::Config::default_table_17_1();
        let s = toml::to_string(&default).unwrap();
        f.write_all(s.as_bytes()).unwrap();
        let args = Args {
            default_config: cfg_path,
            override_config: None,
            data_dir: Some(dir.path().join("data")),
            admin_listen: "127.0.0.1:0".into(),
            rpc_listen: "127.0.0.1:0".into(),
        };
        let cfg = NodeConfig::load(&args).unwrap();
        assert_eq!(cfg.consensus, default);
        assert_eq!(cfg.storage.path, dir.path().join("data"));
    }
}
