//! Layered profile loader (spec §3.1 / §3.4).
//!
//! Layer order (last wins per key):
//!   1. base — `config/default.toml` (consensus tables)
//!   2. profile — `config/profiles/<profile>.toml`
//!   3. optional `config/local.toml` (gitignored)
//!   4. optional `--override-config <path>`
//!
//! Merge rules:
//!   - Tables: field-wise; later layer's keys overwrite earlier layer's keys.
//!   - Arrays (e.g. `[net].bootstrap`, any `[[peer]]`): later layer **replaces**
//!     the earlier array wholesale. No item-merge, no concat.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;
use toml::Value;

use net::NetConfig;
use storage::StorageConfig;

/// L1 vertex production path selector (06-04 design §5).
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum VertexProtocol {
    /// Distributed propose/partial/aggregate protocol.
    Distributed,
    /// Legacy host-side devnet factory (`L1Driver`).
    #[default]
    DevnetFactory,
}

/// Final parsed profile after all layers have been merged.
#[derive(Debug, Deserialize)]
pub struct ProfileFile {
    /// `[node]` section.
    pub node: NodeSection,
    /// `[net]` section.
    pub net: NetConfig,
    /// Optional `[rocksdb]` storage section.
    #[serde(rename = "rocksdb")]
    pub rocksdb: Option<StorageConfig>,
}

/// `[node]` section of a profile.
#[derive(Debug, Deserialize, Clone)]
pub struct NodeSection {
    /// `"live"` requires a working swarm; `"skeleton"` allows the no-network drop path.
    #[serde(default = "default_network_mode")]
    pub network_mode: String,
    /// `[node.identity]` — how this node's key material is sourced.
    pub identity: NodeIdentityToml,
    /// Path to the bootstrap validator set TOML (relative to repo root or absolute).
    #[serde(default = "default_validator_set_path")]
    pub validator_set_path: PathBuf,
    /// When true, `network_mode=live` no longer requires `--allow-skeleton-network` for L3.
    #[serde(default)]
    pub l3_wire_complete: bool,
    /// When true, spawn the local L1 certified-vertex driver (plan 06b-L1).
    #[serde(default)]
    pub l1_driver_enabled: bool,
    /// When true, build real BLS quorum certs via `dag::cert` (07a).
    #[serde(default)]
    pub l1_real_vertex_certs: bool,
    /// Which L1 vertex production path runs (06-04 design):
    /// `"distributed"` = propose → partials → 2f+1 CV (production);
    /// `"devnet_factory"` = legacy L1Driver fabrication (default).
    #[serde(default)]
    pub vertex_protocol: VertexProtocol,
    /// When true, spawn blob chunk custody + gossip ingress (07b).
    #[serde(default)]
    pub l1_blob_custody_enabled: bool,
    /// Fixed chunk size for blob payload splitting (07b).
    #[serde(default = "default_blob_chunk_size")]
    pub blob_chunk_size_bytes: u32,
    /// When true, publish RS erasure shards instead of sequential chunks (07c).
    #[serde(default)]
    pub l1_erasure_enabled: bool,
    /// RS data shard count (07c).
    #[serde(default = "default_erasure_k")]
    pub erasure_k: u32,
    /// RS total shard count (07c).
    #[serde(default = "default_erasure_n")]
    pub erasure_n: u32,
    /// RS data shard byte size (07c).
    #[serde(default = "default_erasure_data_shard_size")]
    pub erasure_data_shard_size_bytes: u32,
}

fn default_erasure_k() -> u32 {
    4
}

fn default_erasure_n() -> u32 {
    6
}

fn default_erasure_data_shard_size() -> u32 {
    32 * 1024
}

fn default_blob_chunk_size() -> u32 {
    65_536
}

fn default_network_mode() -> String {
    "live".into()
}

fn default_validator_set_path() -> PathBuf {
    PathBuf::from("config/valsets/devnet-4.toml")
}

/// `[node.identity]` — kind plus a textual label used by deterministic key derivation.
#[derive(Debug, Deserialize, Clone, Eq, PartialEq)]
pub struct NodeIdentityToml {
    /// One of `"devnet_seed"` (only supported value today) or `"file"`/`"hsm"` in the future.
    pub kind: String,
    /// Label fed into the deterministic key derivation when `kind = "devnet_seed"`.
    pub label: String,
}

/// Compute `<config_dir>/profiles/<profile>.toml`.
#[must_use]
pub fn profile_path(config_dir: &Path, profile: &str) -> PathBuf {
    config_dir.join("profiles").join(format!("{profile}.toml"))
}

/// Merge all configured layer paths and return the merged `toml::Value` tree.
///
/// Layer order is documented at module level. Missing optional layers are silently
/// skipped; missing required layers (the profile) yield a `read` error from `std::fs`.
pub fn merge_layers(
    config_dir: &Path,
    profile: &str,
    extra_overrides: &[PathBuf],
) -> Result<Value> {
    let mut layers: Vec<PathBuf> = Vec::new();
    let base = config_dir.join("default.toml");
    if base.exists() {
        layers.push(base);
    }
    layers.push(profile_path(config_dir, profile));
    let local = config_dir.join("local.toml");
    if local.exists() {
        layers.push(local);
    }
    layers.extend(extra_overrides.iter().cloned());

    let mut acc = Value::Table(toml::map::Map::new());
    for path in &layers {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("read config layer {}", path.display()))?;
        let v: Value = toml::from_str(&raw)
            .with_context(|| format!("parse config layer {}", path.display()))?;
        merge_toml(&mut acc, v);
    }
    Ok(acc)
}

/// Read all configured layer paths and return the merged `ProfileFile`.
///
/// `extra_overrides` carries paths from `--override-config` (in order).
pub fn load_layered(
    config_dir: &Path,
    profile: &str,
    extra_overrides: &[PathBuf],
) -> Result<ProfileFile> {
    let merged = merge_layers(config_dir, profile, extra_overrides)?;
    let parsed: ProfileFile = merged
        .try_into()
        .context("merged config does not match ProfileFile schema")?;
    Ok(parsed)
}

/// Recursive merge: tables are merged field-wise; everything else (arrays,
/// scalars) is **replaced** wholesale by the later layer.
fn merge_toml(dst: &mut Value, src: Value) {
    match (dst, src) {
        (Value::Table(d), Value::Table(s)) => {
            for (k, v) in s {
                match d.get_mut(&k) {
                    Some(slot) => merge_toml(slot, v),
                    None => {
                        d.insert(k, v);
                    }
                }
            }
        }
        (slot, other) => *slot = other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let p = dir.join(name);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut f = fs::File::create(&p).unwrap();
        f.write_all(body.as_bytes()).unwrap();
        p
    }

    fn minimal_profile_body(label: &str, bootstrap: &str) -> String {
        format!(
            r#"
[node]
network_mode = "live"

[node.identity]
kind = "devnet_seed"
label = "{label}"

[net]
listen = ["/ip4/0.0.0.0/tcp/9000"]
bootstrap = [{bootstrap}]

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

    #[test]
    fn parses_minimum_profile() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "profiles/devnet.toml",
            &minimal_profile_body("fixture-node", ""),
        );
        let parsed = load_layered(dir.path(), "devnet", &[]).unwrap();
        assert_eq!(parsed.node.identity.label, "fixture-node");
    }

    #[test]
    fn later_layer_replaces_arrays_wholesale() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "profiles/devnet.toml",
            &minimal_profile_body("a", r#""/dns4/x/tcp/9000""#),
        );
        write(
            dir.path(),
            "local.toml",
            r#"
[net]
bootstrap = ["/dns4/y/tcp/9000"]
"#,
        );
        let parsed = load_layered(dir.path(), "devnet", &[]).unwrap();
        // Array replaced wholesale: only the override entry survives.
        assert_eq!(parsed.net.bootstrap.len(), 1);
        assert!(parsed.net.bootstrap[0].contains("/dns4/y/"));
    }

    #[test]
    fn later_layer_merges_tables_fieldwise() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "profiles/devnet.toml",
            &minimal_profile_body("a", ""),
        );
        write(
            dir.path(),
            "local.toml",
            r#"
[node.identity]
label = "node3"
"#,
        );
        let parsed = load_layered(dir.path(), "devnet", &[]).unwrap();
        // `kind` survives from the profile; only `label` is overwritten.
        assert_eq!(parsed.node.identity.kind, "devnet_seed");
        assert_eq!(parsed.node.identity.label, "node3");
    }

    #[test]
    fn override_config_path_wins_over_local() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "profiles/devnet.toml",
            &minimal_profile_body("a", ""),
        );
        write(
            dir.path(),
            "local.toml",
            "[node.identity]\nlabel = \"from-local\"\n",
        );
        let override_path = write(
            dir.path(),
            "extra.toml",
            "[node.identity]\nlabel = \"from-override\"\n",
        );
        let parsed = load_layered(dir.path(), "devnet", &[override_path]).unwrap();
        assert_eq!(parsed.node.identity.label, "from-override");
    }
}
