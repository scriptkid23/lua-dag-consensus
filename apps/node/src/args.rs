//! CLI arguments for the node binary.

use std::path::PathBuf;

use clap::Parser;

/// LUA-DAG validator node.
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Path to the workspace `config/default.toml` (always loaded first).
    #[arg(long, default_value = "config/default.toml")]
    pub default_config: PathBuf,

    /// Optional override TOML, merged on top of `default_config`.
    #[arg(long)]
    pub override_config: Option<PathBuf>,

    /// Override the data directory (`RocksDB`) from the CLI.
    #[arg(long)]
    pub data_dir: Option<PathBuf>,

    /// Override the prometheus + readiness HTTP bind address.
    #[arg(long, default_value = "127.0.0.1:9100")]
    pub admin_listen: String,

    /// Override the JSON-RPC bind address.
    #[arg(long, default_value = "127.0.0.1:9200")]
    pub rpc_listen: String,
}
