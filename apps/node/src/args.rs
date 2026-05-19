//! CLI arguments for the node binary.

use std::path::PathBuf;

use clap::Parser;

/// LUA-DAG validator node.
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Deployment profile selecting `profiles/<profile>.toml` under config dir.
    #[arg(long, default_value = "devnet", env = "LUA_DAG_PROFILE")]
    pub profile: String,

    /// Directory containing `default.toml`, `profiles/`, optional `local.toml`.
    #[arg(long, default_value = "config", env = "LUA_DAG_CONFIG_DIR")]
    pub config_dir: PathBuf,

    /// Optional extra TOML layers merged after `local.toml` (last wins).
    #[arg(long = "override-config", value_name = "PATH")]
    pub override_configs: Vec<PathBuf>,

    /// Per-container identity label (Compose). Overrides `[node.identity].label`.
    #[arg(long, env = "LUA_DAG_NODE_IDENTITY_LABEL")]
    pub identity_label: Option<String>,

    /// Dev escape hatch — allows skeleton mode that drops broadcasts.
    /// Without this flag, `network_mode = "live"` requires a working swarm.
    #[arg(long, hide = true)]
    pub allow_skeleton_network: bool,

    /// Override the data directory (`RocksDB`) from the CLI.
    #[arg(long, env = "STORAGE_PATH")]
    pub data_dir: Option<PathBuf>,

    /// Override the prometheus + readiness HTTP bind address.
    #[arg(long, default_value = "127.0.0.1:9100")]
    pub admin_listen: String,

    /// Override the JSON-RPC bind address.
    #[arg(long, default_value = "127.0.0.1:9200")]
    pub rpc_listen: String,

    /// Internal: invoke as a local healthcheck probe.
    ///
    /// Performs a single `GET /readyz` against `127.0.0.1:9100` (the
    /// container-side admin port) and exits 0 on `200`, non-zero otherwise.
    /// Used by the Docker `HEALTHCHECK` so the runtime image does not need
    /// a separate `curl` install.
    #[arg(long, hide = true)]
    pub health_probe: bool,
}
