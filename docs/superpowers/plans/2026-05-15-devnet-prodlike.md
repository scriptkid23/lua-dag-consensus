# Prod-like devnet (`node`, layered config, libp2p gossip) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a **4-node Compose devnet** that runs **`apps/node`** with **`[net]`** loaded from layered TOML, **deterministic libp2p identity** per node, **TCP/Noise/Yamux** transport, a **live gossipsub swarm** wired to **`consensus::Event`/`Action`**, **fail-closed defaults** unless **skeleton** mode is explicitly opted in, **Docker/CI aligned to Rust 1.88**, and **remove `lua_dag_smoke`** as the primary image path (**spec binding (a)**). Spec: **`docs/superpowers/specs/2026-05-15-devnet-prodlike-design.md`**.

**Architecture:** **Configuration** merges TOML layers in fixed order (**tables**: field-wise last-wins; **arrays**: later file **replaces** the entire array — no item-wise merge — per spec §3.1). **`LUA_DAG_BOOTSTRAP_PEERS`** (optional) **replaces** the merged `[net].bootstrap` list verbatim after file merge (**spec §3.2 recommendation (a)**). **`net`** exposes a **tcp-only transport builder** plus a **`GossipRuntime`**/`Swarm` driver (crate `net`): subscribe **all stable topics** from `crate::gossip::Topic`, decode inbound payloads **per topic** via `gossipsub::Message::data`, map to **`consensus::Event`**, send on **`events_tx`**. Outbound **broadcast `Action`** variants map **1:1 to topic + payload = `borsh` of inner value** (`encode_action_payload`). **`apps/node`** spawns: admin HTTP (**`/readyz`** becomes readiness-gated when `network_mode=live`), gossip runtime, **`Orchestrator`**, shutdown fan-out. **`--allow-skeleton-network`** (CLI) exposes the old skeleton drop path **only for tests/specific debugging** — default **`devnet` profile refuses** skeleton without this flag (**spec §8**).

**Tech Stack:** Rust **1.88**, **`tokio`**, **`libp2p`** 0.55 (reuse workspace features — add **`relay`**/`request-response` later if needed — **NOT** devnet-required), **`toml`** / **`serde`**, **`borsh`**, **`clap`**, **`axum`** admin, **`rocksdb`** + existing `Dockerfile` builder toolchain, **`docker-compose`**.

---

## Binding decisions (frozen for this implementation)

| Item | Decision |
|------|-----------|
| `lua_dag_smoke` | **DELETE** workspace member + `tools/lua_dag_smoke/`, Dockerfile default target **`node`** only; migrate docker workflow (spec §9 phase 3 **(a)**). |
| Array merge `[net].bootstrap`, etc. | **Tables** merge field-wise across layers; **arrays** are **replaced wholesale** by the later layer (spec §3.1). `LUA_DAG_BOOTSTRAP_PEERS` performs the same whole-list replace post-merge (spec §3.2 recommendation **(a)**). Implemented in Task 3's `merge_toml`. |
| `LUA_DAG_BOOTSTRAP_PEERS` | **Replace** merged list (comma-separated). Empty string ⇒ treat as absent (keep file-derived list). |
| Dev identity | **`[node.identity]`** → `kind = "devnet_seed"` + `label = "...")` derives **Ed25519** key via **`crypto::hash::blake3_with_dst`** (new DST `DEVNET_PEER_IDENTITY`) feeding **`libp2p::identity::Keypair::ed25519_from_bytes(&mut [...])`** (clamp/retry documented in Task 7 if `Err` ever occurs — deterministic labels must be chosen once in tests). Compose sets **`LUA_DAG_NODE_IDENTITY_LABEL`** per service to **`node0`…`node3`** so **one checked-in profile** suffices. |
| Transport | **`build_transport_tcp_only`** — no QUIC mux in swarm for **devnet** profile (still may depend on QUIC-free libp2p build flags if needed later). Existing `build_transport` remains for callers that still want QUIC+TCP until deprecated. |
| Bootstrap wire | Multiaddr is `/dns4/<hostname>/tcp/<port>/p2p/<PeerID>` — Noise is negotiated by the transport after dial and **never** appears in the multiaddr. `/ip4/<dns-name>` is invalid (requires literal IP); use `/dns4/` for container service names. |
| Integration test preference | Two swarms on `127.0.0.1:0` loopback TCP inside `crates/net/tests/devnet_loopback_gossip.rs`; one publishes a `MicroQc`, the other asserts `Event::MicroQcAssembled` arrives. No QUIC, no Docker. |

---

## File structure map

```
crates/crypto/src/hash.rs                                # ADD DST `DEVNET_PEER_IDENTITY`
crates/net/src/transport.rs                              # ADD `build_transport_tcp_only`
crates/net/src/deterministic_key.rs                      # NEW: `devnet_keypair_from_label`
crates/net/src/gossip_wire.rs                            # NEW: Action <-> (Topic, payload), inbound -> Event
crates/net/src/swarm_runner.rs                           # NEW: spawn_gossip_tasks, swarm poll loop, readiness watch
crates/net/src/lib.rs                                    # expose new modules
crates/net/tests/devnet_loopback_gossip.rs               # NEW: 2-node loopback MicroQc round-trip
crates/net/tests/devnet_identity_golden.rs               # NEW: golden PeerIDs for node0..node3
apps/node/src/args.rs                                    # add --profile/--config-dir/--override-config/--allow-skeleton-network/--identity-label
apps/node/src/config.rs                                  # call layered loader, apply env whitelist, identity validation
apps/node/src/config_layers.rs                           # NEW: layered TOML loader with array-replace merge
apps/node/src/runtime.rs                                 # spawn swarm, fan-in events, fail-closed guard
apps/node/src/orchestrator.rs                            # route broadcast vs local actions via gossip_wire::is_broadcast
apps/node/src/observability/health.rs                    # /readyz gated on net_ready watch
apps/node/src/bin/print_devnet_peer_ids.rs               # NEW: prints PeerIDs for the four devnet labels
apps/node/Cargo.toml                                     # register print_devnet_peer_ids bin; add tempfile dev-dep if missing
apps/node/tests/start_fails_closed_in_live_mode.rs       # NEW: spec §8 negative test
config/profiles/devnet.toml                              # NEW: [node]/[net]/[rocksdb] for devnet
config/default.toml                                      # KEEP as consensus base layer (unchanged in this plan)
docker-compose.yml                                       # 4 services, host port mapping table, bootstrap env
Dockerfile                                               # rust:1.88 builder, non-root runtime, copy config tree
docker/README.md                                         # Phase B completion + healthcheck strategy note
README.md                                                # "run devnet" quickstart + env whitelist
.github/workflows/ci.yml                                 # toolchain 1.88 across fmt/clippy/build/test
.github/workflows/docker-smoke.yml                       # single-replica readiness + 4-node compose smoke
Cargo.toml                                               # REMOVE tools/lua_dag_smoke from workspace members
```

Remove tree: **`tools/lua_dag_smoke/`** entirely.

---

### Task 1: Remove Phase-A smoke crate + workspace member (**binding (a)**)

**Files:**

- Delete: `tools/lua_dag_smoke/`
- Modify: root `Cargo.toml` (`members`)

- [ ] **Step 1: Delete the smoke crate**

Delete directory `tools/lua_dag_smoke/` recursively (all files).

- [ ] **Step 2: Remove workspace member**

In root `Cargo.toml`, remove `"tools/lua_dag_smoke"` from `[workspace.members]`.

```toml
members  = [
    "crates/types",
    "crates/crypto",
    "crates/consensus",
    "crates/net",
    "crates/storage",
    "apps/node",
    "apps/sim",
    "apps/cli",
]
```

- [ ] **Step 3: Verify workspace parses**

Run:

```bash
cargo metadata --locked --format-version 1 --no-deps
```

Expected: **exit code 0**

- [ ] **Step 4: Commit**

Run:

```bash
git add Cargo.toml
git rm -r tools/lua_dag_smoke
git commit -m "build: drop lua_dag_smoke workspace crate for prod-like devnet"
```

---

### Task 2: Add crypto DST constant for deterministic dev identities

**Files:**

- Modify: `crates/crypto/src/hash.rs`

- [ ] **Step 1: Append DST**

```rust
    /// Deterministic peer key derivation for **`devnet` only** —
    /// blake3 input is hashed with this prefix before supplying bytes
    /// to libp2p Ed25519 key material.
    pub const DEVNET_PEER_IDENTITY: &[u8] = b"lua-dag/v1/devnet-peer-identity";
```

Place inside `pub mod dst { ... }` next to adjacent constants.

- [ ] **Step 2: Add regression test proving uniqueness against every existing DST**

Still in `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn devnet_peer_identity_dst_is_unique() {
        use std::collections::HashSet;
        let ids: &[&[u8]] = &[
            dst::CONTENT_HASH,
            dst::MICRO_QC,
            dst::MACRO_PROPOSAL,
            dst::MACRO_VOTE,
            dst::BEACON,
            dst::SUBNET_ASSIGN,
            dst::POP,
            dst::DEVNET_PEER_IDENTITY,
        ];
        let set: HashSet<&[u8]> = ids.iter().copied().collect();
        assert_eq!(set.len(), ids.len(), "DST registry has a duplicate");
    }
```

If new DSTs are added later, extend the list — the test is the canonical "no duplicate DSTs" check.

- [ ] **Step 3: Run scoped test**

Run:

```bash
cargo test -p crypto hash::tests::devnet_peer_identity_dst_is_unique --locked -- --nocapture
```

Expected: **PASS**

- [ ] **Step 4: Commit** (`feat(crypto): add DEVNET_PEER_IDENTITY DST`)

---

### Task 3: Layered TOML loader (spec §3.1) — profile file + merge semantics

**Rationale:** `config/default.toml` stays a **`consensus::Config`** document. Network/ops data lives in **`config/profiles/<profile>.toml`**. The loader implements the spec's layered merge — base + profile + optional `config/local.toml` + optional `--override-config` — with **arrays replaced wholesale** and **tables merged field-wise**.

**Files:**

- Create: `apps/node/src/config_layers.rs`

- [ ] **Step 1: Add `apps/node/src/config_layers.rs`**

```rust
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

/// Final parsed profile after all layers have been merged.
#[derive(Debug, Deserialize)]
pub struct ProfileFile {
    pub node: NodeSection,
    pub net: NetConfig,
    #[serde(rename = "rocksdb")]
    pub rocksdb: Option<StorageConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct NodeSection {
    #[serde(default = "default_network_mode")]
    pub network_mode: String,
    pub identity: NodeIdentityToml,
}

fn default_network_mode() -> String {
    "live".into()
}

#[derive(Debug, Deserialize, Clone, Eq, PartialEq)]
pub struct NodeIdentityToml {
    pub kind: String,
    pub label: String,
}

#[must_use]
pub fn profile_path(config_dir: &Path, profile: &str) -> PathBuf {
    config_dir.join("profiles").join(format!("{profile}.toml"))
}

/// Read all configured layer paths and return the merged `toml::Value` tree.
///
/// `extra_overrides` carries paths from `--override-config` (in order).
pub fn load_layered(
    config_dir: &Path,
    profile: &str,
    extra_overrides: &[PathBuf],
) -> Result<ProfileFile> {
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

    let mut acc = Value::Table(Default::default());
    for path in &layers {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("read config layer {}", path.display()))?;
        let v: Value = toml::from_str(&raw)
            .with_context(|| format!("parse config layer {}", path.display()))?;
        merge_toml(&mut acc, v);
    }

    let parsed: ProfileFile = acc
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
```

Wire `mod config_layers;` from the module tree that already declares `mod config;` in **`apps/node`**.

In **`NodeConfig::load`**, validate **`node.identity.kind == "devnet_seed"`** unless **`--allow-skeleton-network`**.

- [ ] **Step 2: Add `#[cfg(test)] mod tests` to `config_layers.rs`**

```rust
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
        write(dir.path(), "profiles/devnet.toml", &minimal_profile_body("fixture-node", ""));
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
        assert!(parsed.net.bootstrap[0].to_string().contains("/dns4/y/"));
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
        write(dir.path(), "local.toml", "[node.identity]\nlabel = \"from-local\"\n");
        let override_path = write(
            dir.path(),
            "extra.toml",
            "[node.identity]\nlabel = \"from-override\"\n",
        );
        let parsed = load_layered(dir.path(), "devnet", &[override_path]).unwrap();
        assert_eq!(parsed.node.identity.label, "from-override");
    }
}
```

Add **`tempfile`** to **`apps/node`** **dev-dependencies** if not already listed.

- [ ] **Step 3: Run tests**

```bash
cargo test -p node config_layers::tests --locked -- --nocapture
```

Expected: **PASS** (all four cases).

- [ ] **Step 4: Commit** (`feat(node): layered TOML loader with array-replace merge`)

---

### Task 4: Wire **`NodeConfig::load`** → layered paths + env whitelist

**Files:**

- Modify: `apps/node/src/config.rs`
- Modify: `apps/node/src/args.rs`

**Precedence rule (single source of truth):**

For each CLI flag, the order is **explicit CLI > env > built-in default**. `clap`'s `env(..)` attribute encodes this directly — no extra logic needed in `NodeConfig::load`.

- [ ] **Step 1: Update `apps/node/src/args.rs`**

```rust
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
```

`STORAGE_PATH` continues to be sourced via the existing `--data-dir` arg with `env = "STORAGE_PATH"` (add the env attribute if not already present).

- [ ] **Step 2: Update `apps/node/src/config.rs` — env whitelist + bootstrap replace**

After `load_layered(...)` returns the merged `ProfileFile`:

```rust
// LUA_DAG_NODE_IDENTITY_LABEL overrides `[node.identity].label`.
if let Some(label) = args.identity_label.as_deref() {
    cfg.node.identity.label = label.to_string();
}

// LUA_DAG_BOOTSTRAP_PEERS replaces the merged bootstrap list wholesale
// (spec §3.2 recommendation (a)). Empty string is treated as unset.
if let Ok(raw) = std::env::var("LUA_DAG_BOOTSTRAP_PEERS") {
    let trimmed = raw.trim();
    if !trimmed.is_empty() {
        cfg.net.bootstrap = trimmed
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.parse().with_context(|| format!("bootstrap multiaddr `{s}`")))
            .collect::<Result<Vec<_>>>()?;
    }
}
```

**Env whitelist (no other env vars are read for config):**

| Var | Purpose |
|-----|---------|
| `LUA_DAG_PROFILE` | profile name (default `devnet`) |
| `LUA_DAG_CONFIG_DIR` | config root dir (default `config`) |
| `LUA_DAG_NODE_IDENTITY_LABEL` | per-container identity label |
| `LUA_DAG_BOOTSTRAP_PEERS` | comma-separated multiaddrs; replaces merged `[net].bootstrap` |
| `STORAGE_PATH` | RocksDB path (via existing `--data-dir`) |

- [ ] **Step 3: Identity-kind validation**

After load, refuse to start if the identity kind is unknown unless skeleton is enabled:

```rust
match (cfg.node.identity.kind.as_str(), args.allow_skeleton_network) {
    ("devnet_seed", _) => {}
    (other, true) => tracing::warn!(kind = other, "unknown identity kind; allowed by --allow-skeleton-network"),
    (other, false) => anyhow::bail!(
        "unknown node.identity.kind `{other}`; only `devnet_seed` is supported for live mode"
    ),
}
```

The transport-level fail-closed gate is implemented in Task 12; this step only validates the parsed config shape.

- [ ] **Step 4: Unit test the env precedence**

In `apps/node/src/config.rs` tests:

```rust
#[test]
fn bootstrap_env_replaces_file_bootstrap() {
    let _g = lock_env(); // serial test guard if your test suite uses one
    std::env::set_var("LUA_DAG_BOOTSTRAP_PEERS", "/dns4/x/tcp/9000,/dns4/y/tcp/9000");
    // ... build cfg via load_layered + apply env step ...
    std::env::remove_var("LUA_DAG_BOOTSTRAP_PEERS");
    assert_eq!(cfg.net.bootstrap.len(), 2);
}
```

If no env-test guard exists, mark this test `#[ignore]` and document running it with `--test-threads=1`. (Or use a crate like `temp-env` — implementer's call; do not invent fragile global state.)

- [ ] **Step 5: Commit** (`feat(node): wire layered loader, env whitelist, identity validation`)

---

### Task 5: Checked-in **`config/profiles/devnet.toml`** + PeerID priming

**Files:**

- Create: `config/profiles/devnet.toml`
- Create: `apps/node/src/bin/print_devnet_peer_ids.rs` (small companion binary)
- Modify: `apps/node/Cargo.toml` (register the bin)
- Modify: `config/README.md` if it mentions only `local.toml` (optional touch-up)

- [ ] **Step 1: Write `config/profiles/devnet.toml`**

Bootstrap stays empty in the profile — Compose injects `LUA_DAG_BOOTSTRAP_PEERS` so the four-node topology lives in one place.

```toml
[node]
network_mode = "live"

[node.identity]
kind = "devnet_seed"
# Compose sets LUA_DAG_NODE_IDENTITY_LABEL per service; the bare-metal default below
# is only used when running outside Compose.
label = "node0"

[net]
listen = [ "/ip4/0.0.0.0/tcp/9000" ]
bootstrap = []

[net.gossip]
heartbeat_ms = 700
mesh_n = 8
mesh_n_low = 6
mesh_n_high = 12

[net.peers]
max_peers = 64
ban_duration_secs = 600

[rocksdb]
path = "/data/rocksdb"
create_if_missing = true
max_total_wal_size_mb = 256
```

- [ ] **Step 2: Add `print_devnet_peer_ids` companion binary (resolves the PeerID chicken-and-egg)**

`apps/node/src/bin/print_devnet_peer_ids.rs`:

```rust
//! Prints the deterministic devnet PeerIDs for labels `node0`..`node3`.
//! Use the output to populate `LUA_DAG_BOOTSTRAP_PEERS` in `docker-compose.yml`
//! and the golden literals in the integration test (Task 7 step 3).

use net::deterministic_key::devnet_keypair_from_label;

fn main() {
    for label in ["node0", "node1", "node2", "node3"] {
        let kp = devnet_keypair_from_label(label).expect("derive key");
        println!("{label} {}", kp.public().to_peer_id());
    }
}
```

Register in `apps/node/Cargo.toml`:

```toml
[[bin]]
name = "print_devnet_peer_ids"
path = "src/bin/print_devnet_peer_ids.rs"
```

- [ ] **Step 3: Compute, paste, freeze**

After Task 7 ships, run once:

```bash
cargo run -p node --bin print_devnet_peer_ids --locked
```

Paste the four PeerIDs into:

1. `docker-compose.yml` `LUA_DAG_BOOTSTRAP_PEERS` for each service (see Task 13).
2. The golden-PeerID asserts in `crates/net/tests/devnet_identity_golden.rs` (Task 7 step 3).

The keys are derived from the BLAKE3 DST + label and are stable across machines — they only need to be computed once and committed.

**Compose env pattern (final, after PeerIDs are pasted):**

```yaml
services:
  node0:
    environment:
      LUA_DAG_PROFILE: devnet
      LUA_DAG_NODE_IDENTITY_LABEL: node0
      # PeerIDs from `cargo run --bin print_devnet_peer_ids`. node0 dials the other three.
      LUA_DAG_BOOTSTRAP_PEERS: "/dns4/node1/tcp/9000/p2p/<PeerId1>,/dns4/node2/tcp/9000/p2p/<PeerId2>,/dns4/node3/tcp/9000/p2p/<PeerId3>"
```

- [ ] **Step 4: Commit** (`feat(node): devnet profile + PeerID priming binary`)

---

### Task 6: **`build_transport_tcp_only`**

**Files:**

- Modify: `crates/net/src/transport.rs`

- [ ] **Step 1: Append the TCP-only transport builder**

```rust
/// QUIC-free transport for Compose + CI (spec §4.1 — devnet TCP baseline).
pub fn build_transport_tcp_only(keypair: &Keypair) -> Result<Boxed<(PeerId, StreamMuxerBox)>> {
    let tcp_transport = tcp::tokio::Transport::new(tcp::Config::new().nodelay(true));
    let noise = noise::Config::new(keypair).map_err(|e| Error::Transport(e.to_string()))?;
    let yamux = yamux::Config::default();
    Ok(tcp_transport
        .upgrade(upgrade::Version::V1Lazy)
        .authenticate(noise)
        .multiplex(yamux)
        .boxed())
}
```

The existing `build_transport` (TCP + QUIC) remains for now; do **not** delete it in this task.

- [ ] **Step 2: Re-export if needed**

If `transport` is not `pub mod`, ensure `build_transport_tcp_only` is reachable at `net::build_transport_tcp_only` or `net::transport::build_transport_tcp_only`. Match the existing public surface of `build_transport`.

- [ ] **Step 3: Compile check**

```bash
cargo check -p net --locked
```

Expected: clean.

- [ ] **Step 4: Commit** (`feat(net): TCP-only transport builder for devnet`)

---

### Task 7: **`devnet_keypair_from_label`** + golden PeerID test

**Files:**

- Create: `crates/net/src/deterministic_key.rs`
- Modify: `crates/net/src/lib.rs`
- Create: `crates/net/tests/devnet_identity_golden.rs`

- [ ] **Step 1: Create `crates/net/src/deterministic_key.rs`**

```rust
//! Deterministic (dev-only) libp2p keys from textual labels (spec §3.4).

use crypto::hash::{blake3_with_dst, dst};
use libp2p::identity::Keypair;
use types::crypto_types::Hash32;

use crate::{Error, Result};

/// Derives a libp2p Ed25519 key deterministically from a textual label.
///
/// Use **only** for the `devnet` profile. Never wire this into testnet/prod
/// — those profiles MUST mount real keypairs (spec §3.4 option 2).
///
/// Collision resistance comes from BLAKE3 + the fixed DST separator
/// (`dst::DEVNET_PEER_IDENTITY`).
pub fn devnet_keypair_from_label(label: &str) -> Result<Keypair> {
    let Hash32(mut bytes) = blake3_with_dst(dst::DEVNET_PEER_IDENTITY, label.as_bytes());
    // ed25519_from_bytes on a 32-byte seed cannot fail for any input the BLAKE3
    // output can produce; the `Result` exists for API symmetry. If a future
    // libp2p version tightens validation, surface it as a Transport error.
    Keypair::ed25519_from_bytes(&mut bytes)
        .map_err(|e| Error::Transport(format!("ed25519 key rejected for label `{label}`: {e}")))
}
```

Add `pub mod deterministic_key;` to `crates/net/src/lib.rs` and re-export `devnet_keypair_from_label` if the existing public surface uses crate-level re-exports.

- [ ] **Step 2: Inline stability test in the module**

In `crates/net/src/deterministic_key.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_label_yields_same_peer_id() {
        let a = devnet_keypair_from_label("node0").unwrap();
        let b = devnet_keypair_from_label("node0").unwrap();
        assert_eq!(a.public().to_peer_id(), b.public().to_peer_id());
    }

    #[test]
    fn different_labels_yield_different_peer_ids() {
        let a = devnet_keypair_from_label("node0").unwrap();
        let b = devnet_keypair_from_label("node1").unwrap();
        assert_ne!(a.public().to_peer_id(), b.public().to_peer_id());
    }
}
```

- [ ] **Step 3: Golden PeerID test** (`crates/net/tests/devnet_identity_golden.rs`)

This file pins the four devnet PeerIDs so the Compose bootstrap list cannot drift out of sync silently.

```rust
//! Golden PeerIDs for `node0`..`node3` — must match the values used in
//! `docker-compose.yml` LUA_DAG_BOOTSTRAP_PEERS.

use net::deterministic_key::devnet_keypair_from_label;

// FILL ME after Task 5 step 3 (run `cargo run --bin print_devnet_peer_ids`).
const GOLDEN: &[(&str, &str)] = &[
    ("node0", "<PASTE PEER ID HERE>"),
    ("node1", "<PASTE PEER ID HERE>"),
    ("node2", "<PASTE PEER ID HERE>"),
    ("node3", "<PASTE PEER ID HERE>"),
];

#[test]
fn golden_peer_ids_are_stable() {
    for (label, expected) in GOLDEN {
        let kp = devnet_keypair_from_label(label).unwrap();
        assert_eq!(
            kp.public().to_peer_id().to_string(),
            *expected,
            "PeerID for label `{label}` drifted — regenerate compose bootstrap"
        );
    }
}
```

Initially the placeholders make this test fail; that is intentional. After Task 5 step 3 the literals are pasted and the test goes green.

- [ ] **Step 4: Run module tests** (golden test is expected to fail until the literals are pasted)

```bash
cargo test -p net deterministic_key::tests --locked -- --nocapture
```

- [ ] **Step 5: Commit** (`feat(net): deterministic devnet PeerID derivation`)

---

### Task 8: **`gossip_wire`** — topic ↔ event/action mapping

**Files:**

- Create: `crates/net/src/gossip_wire.rs`
- Modify: `crates/net/src/lib.rs` (add `pub mod gossip_wire;`)

Two total functions: outbound translates a consensus `Action` to a `(Topic, payload)`; inbound translates a `(topic_str, payload)` to a consensus `Event`. Both return `Ok(None)` only for variants that intentionally have no wire counterpart — never silently for serialization failures.

- [ ] **Step 1: Write `crates/net/src/gossip_wire.rs`**

```rust
//! Bridges consensus `Action` ↔ gossip topics + Borsh payloads.

use consensus::action::Action;
use consensus::event::{Event, SubnetId};
use types::macros::{MacroProposal, MacroQc};
use types::micro::MicroQc;
use types::slashing::SlashEvidence;
use consensus::event::{BlsPartial, SubnetAggregate};

use crate::error::Result;
use crate::gossip::Topic;
use crate::gossip::codec::{decode_event_payload, encode_action_payload};

const TOPIC_CERTIFIED_VERTEX: &str = "lua-dag/v1/certified-vertex";
const TOPIC_MICRO_QC: &str = "lua-dag/v1/micro-qc";
const TOPIC_MACRO_PROPOSAL: &str = "lua-dag/v1/macro-proposal";
const TOPIC_SUBNET_AGGREGATE: &str = "lua-dag/v1/subnet-aggregate";
const TOPIC_MACRO_QC: &str = "lua-dag/v1/macro-qc";
const TOPIC_SLASH_EVIDENCE: &str = "lua-dag/v1/slash-evidence";
const TOPIC_BLS_PARTIAL_PREFIX: &str = "lua-dag/v1/bls-partial/";

/// Map a consensus `Action` to its gossip topic + Borsh payload.
///
/// Returns `Ok(None)` for actions that are intentionally host-local
/// (timers, persistence, blob status). Returns `Err` if encoding fails —
/// never drop a broadcast silently.
pub fn outbound_broadcast(action: &Action) -> Result<Option<(Topic, Vec<u8>)>> {
    let pair = match action {
        Action::BroadcastMicroQc(m) => (Topic::MicroQc, encode_action_payload(m)?),
        Action::BroadcastMacroProposal(m) => (Topic::MacroProposal, encode_action_payload(m)?),
        Action::BroadcastBlsPartial(p) => (Topic::BlsPartial(p.subnet), encode_action_payload(p)?),
        Action::BroadcastSubnetAggregate(a) => (Topic::SubnetAggregate, encode_action_payload(a)?),
        Action::BroadcastMacroQc(q) => (Topic::MacroQc, encode_action_payload(q)?),
        Action::EmitSlashEvidence { evidence, .. } => {
            (Topic::SlashEvidence, encode_action_payload(evidence)?)
        }
        Action::PersistMacroQc(_)
        | Action::ScheduleTimer { .. }
        | Action::CancelTimer(_)
        | Action::UpdateBlobStatus { .. } => return Ok(None),
    };
    Ok(Some(pair))
}

/// Returns `true` iff this action would have been published by `outbound_broadcast`.
///
/// Cheap pre-flight used by the orchestrator to route broadcast actions onto
/// the gossip channel and keep timer/persistence actions on the local path.
#[must_use]
pub fn is_broadcast(action: &Action) -> bool {
    matches!(
        action,
        Action::BroadcastMicroQc(_)
            | Action::BroadcastMacroProposal(_)
            | Action::BroadcastBlsPartial(_)
            | Action::BroadcastSubnetAggregate(_)
            | Action::BroadcastMacroQc(_)
            | Action::EmitSlashEvidence { .. }
    )
}

/// Map an inbound gossipsub message to a consensus `Event`.
///
/// Returns `Ok(None)` for topics we subscribe to but do not yet have an
/// `Event` mapping for (e.g. `CertifiedVertex` if upstream feed is added
/// later). Returns `Err` on decode failure — callers may log and continue
/// rather than terminate the swarm.
pub fn inbound_message(topic_str: &str, data: &[u8]) -> Result<Option<Event>> {
    if topic_str == TOPIC_MICRO_QC {
        let m: MicroQc = decode_event_payload(data)?;
        // No dedicated `MicroQcReceived` exists today; surface as Assembled.
        // (Pending alignment with consensus; see plan §10 for owner.)
        return Ok(Some(Event::MicroQcAssembled(m)));
    }
    if topic_str == TOPIC_MACRO_PROPOSAL {
        let m: MacroProposal = decode_event_payload(data)?;
        return Ok(Some(Event::MacroProposalReceived(m)));
    }
    if topic_str == TOPIC_SUBNET_AGGREGATE {
        let a: SubnetAggregate = decode_event_payload(data)?;
        return Ok(Some(Event::SubnetAggregateReceived(a)));
    }
    if topic_str == TOPIC_MACRO_QC {
        let q: MacroQc = decode_event_payload(data)?;
        return Ok(Some(Event::MacroQcReceived(q)));
    }
    if topic_str == TOPIC_SLASH_EVIDENCE {
        let s: SlashEvidence = decode_event_payload(data)?;
        return Ok(Some(Event::SlashEvidenceFound(s)));
    }
    if let Some(rest) = topic_str.strip_prefix(TOPIC_BLS_PARTIAL_PREFIX) {
        let _subnet: u32 = rest.parse().map_err(|e: std::num::ParseIntError| {
            crate::error::Error::Codec(format!("bad subnet id `{rest}`: {e}"))
        })?;
        // SubnetId is preserved inside the payload itself.
        let p: BlsPartial = decode_event_payload(data)?;
        return Ok(Some(Event::BlsPartialReceived(p)));
    }
    if topic_str == TOPIC_CERTIFIED_VERTEX {
        // Mode-A devnet does not produce CertifiedVertex broadcasts; subscribers
        // ignore until L1 ingestion lands.
        return Ok(None);
    }
    Ok(None)
}
```

**Note on `MicroQcAssembled`:** the current `Event` enum has no `MicroQcReceived` variant — only `MicroQcAssembled`. The inbound mapping uses `MicroQcAssembled` as the closest match for devnet smoke. Splitting the variant is **outside this plan's scope** and is captured as a follow-up in the conformance notes; flag it in PR review if it blocks anything.

- [ ] **Step 2: Add unit tests covering each topic round-trip**

The cleanest fixtures come from the consensus crate's existing test factories — search `crates/consensus/src/event.rs`, `crates/types/src/macros.rs`, etc. for `Default` / `mock_*` helpers before fabricating fields by hand.

Minimal smoke test (extend in implementation):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use consensus::action::Action;
    use types::micro::MicroQc;

    #[test]
    fn micro_qc_round_trip() {
        let m = MicroQc::default(); // or test-factory; verify the type implements Default
        let action = Action::BroadcastMicroQc(m.clone());
        let (topic, payload) = outbound_broadcast(&action).unwrap().unwrap();
        let topic_str = topic.ident().to_string();
        let ev = inbound_message(&topic_str, &payload).unwrap().unwrap();
        assert!(matches!(ev, Event::MicroQcAssembled(_)));
    }

    #[test]
    fn timer_action_is_not_broadcast() {
        let action = Action::CancelTimer(consensus::event::TimerId(1));
        assert!(outbound_broadcast(&action).unwrap().is_none());
        assert!(!is_broadcast(&action));
    }
}
```

If `MicroQc::default` does not exist, either add a `#[cfg(test)] pub fn fixture()` helper to `crates/types/src/micro.rs` (preferred — reusable) or compose a minimal value inline. Do not skip the test.

- [ ] **Step 3: Run tests**

```bash
cargo test -p net gossip_wire::tests --locked -- --nocapture
```

Expected: **PASS**.

- [ ] **Step 4: Commit** (`feat(net): gossip wire mapping for consensus events/actions`)

---

### Task 9: **`swarm_runner`** — swarm build + poll loop

**Files:**

- Create: `crates/net/src/swarm_runner.rs`
- Modify: `crates/net/src/lib.rs` (add `pub mod swarm_runner;`)

**Public API:**

```rust
pub struct GossipSpawn {
    /// Inbound events decoded from gossip; orchestrator reads from this.
    pub events_rx: tokio::sync::mpsc::Receiver<consensus::event::Event>,
    /// Readiness signal — flips `true` once every listen addr has bound.
    pub ready: tokio::sync::watch::Receiver<bool>,
    /// Handle to the running swarm task. Drop or await on shutdown.
    pub handle: tokio::task::JoinHandle<()>,
}

/// Spawn the gossipsub swarm task. Returns once listen addrs are queued —
/// readiness flips later on the watch channel.
pub async fn spawn_gossip_tasks(
    keypair: libp2p::identity::Keypair,
    net_cfg: net::NetConfig,
    actions_rx: tokio::sync::mpsc::Receiver<consensus::action::Action>,
) -> anyhow::Result<GossipSpawn>;
```

**Subscription policy (single, frozen decision):**

The devnet swarm subscribes to **every non-parameterized topic** plus **`Topic::BlsPartial(SubnetId(0))`** as a placeholder for Mode-A subnet traffic. Specifically:

- `Topic::CertifiedVertex`
- `Topic::MicroQc`
- `Topic::MacroProposal`
- `Topic::SubnetAggregate`
- `Topic::MacroQc`
- `Topic::SlashEvidence`
- `Topic::BlsPartial(SubnetId(0))`

This satisfies the spec's "no silent drops on broadcast actions" rule for the topics the current `Action` enum can produce. Dynamic per-subnet subscription is a follow-up tied to the Mode-A subnet aggregator — out of scope for this plan.

**Readiness policy (single, frozen decision):**

`ready` flips to `true` once `SwarmEvent::NewListenAddr` has been observed for **every** address in `net_cfg.listen`. Peer-count-based readiness is rejected: Compose nodes need to be ready *before* peers connect so the bootstrap dial loop has somewhere to land.

- [ ] **Step 1: Implementation skeleton** (`crates/net/src/swarm_runner.rs`)

```rust
//! Live libp2p swarm wired to consensus Event/Action channels (spec §4.1).

use anyhow::{Context, Result};
use consensus::action::Action;
use consensus::event::{Event, SubnetId};
use libp2p::{
    gossipsub::{self, MessageAuthenticity},
    identity::Keypair,
    swarm::{NetworkBehaviour, SwarmEvent},
    Multiaddr, Swarm,
};
use std::time::Duration;
use tokio::sync::{mpsc, watch};

use crate::gossip::Topic;
use crate::gossip_wire;
use crate::transport::build_transport_tcp_only;
use net::NetConfig;

const EVENT_BUFFER: usize = 1024;

#[derive(NetworkBehaviour)]
struct DevnetBehaviour {
    gossipsub: gossipsub::Behaviour,
}

pub struct GossipSpawn {
    pub events_rx: mpsc::Receiver<Event>,
    pub ready: watch::Receiver<bool>,
    pub handle: tokio::task::JoinHandle<()>,
}

pub async fn spawn_gossip_tasks(
    keypair: Keypair,
    net_cfg: NetConfig,
    mut actions_rx: mpsc::Receiver<Action>,
) -> Result<GossipSpawn> {
    let transport = build_transport_tcp_only(&keypair)
        .context("build TCP+Noise+Yamux transport")?;

    let gossip_cfg = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(Duration::from_millis(net_cfg.gossip.heartbeat_ms))
        .validation_mode(gossipsub::ValidationMode::Strict)
        .mesh_n(net_cfg.gossip.mesh_n)
        .mesh_n_low(net_cfg.gossip.mesh_n_low)
        .mesh_n_high(net_cfg.gossip.mesh_n_high)
        .build()
        .map_err(anyhow::Error::msg)?;

    let mut gossipsub = gossipsub::Behaviour::new(
        MessageAuthenticity::Signed(keypair.clone()),
        gossip_cfg,
    )
    .map_err(anyhow::Error::msg)?;

    for topic in subscribe_set() {
        gossipsub.subscribe(&topic.ident())
            .with_context(|| format!("subscribe {topic:?}"))?;
    }

    let mut swarm = Swarm::new(
        transport,
        DevnetBehaviour { gossipsub },
        keypair.public().to_peer_id(),
        libp2p::swarm::Config::with_tokio_executor(),
    );

    // Listen on each configured address. Errors here are fatal — we cannot
    // claim live mode without listening.
    let mut pending_listen: std::collections::HashSet<Multiaddr> = Default::default();
    for addr in &net_cfg.listen {
        swarm.listen_on(addr.clone()).with_context(|| format!("listen_on {addr}"))?;
        pending_listen.insert(addr.clone());
    }

    // Dial bootstrap peers (best-effort; failures are logged, not fatal).
    for addr in &net_cfg.bootstrap {
        if let Err(e) = swarm.dial(addr.clone()) {
            tracing::warn!(%addr, error = %e, "bootstrap dial failed");
        }
    }

    let (events_tx, events_rx) = mpsc::channel::<Event>(EVENT_BUFFER);
    let (ready_tx, ready_rx) = watch::channel(false);

    let handle = tokio::spawn(async move {
        use futures::StreamExt;
        loop {
            tokio::select! {
                ev = swarm.select_next_some() => match ev {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        // Strip the /p2p suffix libp2p may append before comparing.
                        let stripped = strip_p2p(&address);
                        pending_listen.retain(|a| strip_p2p(a) != stripped);
                        if pending_listen.is_empty() {
                            let _ = ready_tx.send(true);
                        }
                    }
                    SwarmEvent::Behaviour(DevnetBehaviourEvent::Gossipsub(
                        gossipsub::Event::Message { message, .. },
                    )) => {
                        match gossip_wire::inbound_message(message.topic.as_str(), &message.data) {
                            Ok(Some(event)) => {
                                if events_tx.send(event).await.is_err() {
                                    tracing::warn!("events_rx dropped; shutting swarm task");
                                    break;
                                }
                            }
                            Ok(None) => {} // topic recognized but no Event mapping yet
                            Err(e) => tracing::warn!(error = %e, "inbound decode failed"),
                        }
                    }
                    _ => {}
                },
                maybe_action = actions_rx.recv() => match maybe_action {
                    None => break, // upstream closed
                    Some(action) => match gossip_wire::outbound_broadcast(&action) {
                        Ok(Some((topic, payload))) => {
                            if let Err(e) = swarm.behaviour_mut().gossipsub.publish(topic.ident(), payload) {
                                tracing::warn!(error = %e, ?topic, "gossipsub publish failed");
                            }
                        }
                        Ok(None) => {
                            // Non-broadcast action reached the swarm — orchestrator should
                            // have routed it locally. Log so we notice routing bugs.
                            tracing::debug!(?action, "non-broadcast action reached swarm; ignored");
                        }
                        Err(e) => tracing::warn!(error = %e, ?action, "outbound encode failed"),
                    },
                },
            }
        }
    });

    Ok(GossipSpawn { events_rx, ready: ready_rx, handle })
}

fn subscribe_set() -> [Topic; 7] {
    [
        Topic::CertifiedVertex,
        Topic::MicroQc,
        Topic::MacroProposal,
        Topic::SubnetAggregate,
        Topic::MacroQc,
        Topic::SlashEvidence,
        Topic::BlsPartial(SubnetId(0)),
    ]
}

fn strip_p2p(addr: &Multiaddr) -> Multiaddr {
    let mut out = Multiaddr::empty();
    for proto in addr.iter() {
        if matches!(proto, libp2p::multiaddr::Protocol::P2p(_)) {
            continue;
        }
        out.push(proto);
    }
    out
}
```

If the exact `NetworkBehaviour` derive name (`DevnetBehaviourEvent`) differs from what libp2p 0.55 generates, adjust the match — the macro produces `<BehaviourName>Event` by default.

- [ ] **Step 2: Integration test** (`crates/net/tests/devnet_loopback_gossip.rs`)

Spin up **two** swarms on `127.0.0.1:0` loopback (TCP), have one publish a `MicroQc`, assert the other receives `Event::MicroQcAssembled` within 10s. This is the smallest credible exercise of the full pipeline; QUIC and Docker are out of scope here.

- [ ] **Step 3: Run tests**

```bash
cargo test -p net swarm_runner --locked -- --nocapture
cargo test -p net --test devnet_loopback_gossip --locked -- --nocapture
```

- [ ] **Step 4: Commit** (`feat(net): live gossipsub swarm driver for devnet`)

---

### Task 10: Integrate swarm into **`apps/node`** runtime

**Files:**

- Modify: `apps/node/src/runtime.rs`
- Modify: `apps/node/src/orchestrator.rs`

The orchestrator splits each emitted `Action` into two paths: **broadcasts** go to the swarm's `actions_tx` channel; **local actions** (timers, persistence, blob status) stay on the existing translator. `net::gossip_wire::is_broadcast` is the single classifier.

- [ ] **Step 1: Add a typed action dispatcher to the orchestrator**

In `apps/node/src/orchestrator.rs` (or wherever the action drain currently runs):

```rust
use net::gossip_wire::is_broadcast;

for action in actions {
    if is_broadcast(&action) {
        // Lossy back-pressure: if the swarm is wedged we'd rather drop one
        // broadcast and keep consensus running than deadlock the orchestrator.
        if let Err(e) = self.net_actions_tx.try_send(action) {
            tracing::warn!(error = %e, "net actions channel full; dropping broadcast");
        }
    } else {
        self.bridge.translate_action(action)?;
    }
}
```

Hold `net_actions_tx: tokio::sync::mpsc::Sender<consensus::action::Action>` as a field on the orchestrator (constructor takes it). Delete any code path where `Bridge::translate_action` was silently dropping broadcasts.

- [ ] **Step 2: Wire spawning in `apps/node/src/runtime.rs`**

```rust
// Identity
let keypair = net::deterministic_key::devnet_keypair_from_label(&cfg.node.identity.label)?;

// Channels
let (net_actions_tx, net_actions_rx) =
    tokio::sync::mpsc::channel::<consensus::action::Action>(1024);

// Swarm
let net::swarm_runner::GossipSpawn { mut events_rx, ready, handle: swarm_handle } =
    net::swarm_runner::spawn_gossip_tasks(keypair, cfg.net.clone(), net_actions_rx).await?;

// Orchestrator receives both the existing events channel AND swarm events.
// Fan-in: forward swarm events into the existing events_tx.
let events_tx_for_swarm = events_tx.clone();
tokio::spawn(async move {
    while let Some(ev) = events_rx.recv().await {
        if events_tx_for_swarm.send(ev).await.is_err() { break; }
    }
});

let orchestrator = Orchestrator::new(/* existing args */, net_actions_tx);

// Admin server gets the readiness watch (see Task 11).
let admin_handle = serve_admin(admin_cfg, ready.clone()).await?;
```

On shutdown, await `swarm_handle` after closing channels so the swarm task drains cleanly.

- [ ] **Step 3: Compile**

```bash
cargo check -p node --locked
```

- [ ] **Step 4: Commit** (`feat(node): route broadcasts through gossip swarm`)

---

### Task 11: **`serve_admin`** readiness coupling

**Files:**

- Modify: `apps/node/src/observability/health.rs`

- [ ] **Step 1: Thread the readiness watch into `AdminState`**

Add a `net_ready: tokio::sync::watch::Receiver<bool>` field to `AdminState` and pipe it through `serve_admin`'s signature so Task 10 step 2 can pass `ready.clone()` in.

- [ ] **Step 2: Update `/readyz`**

```rust
async fn readyz(State(state): State<Arc<AdminState>>) -> impl IntoResponse {
    if *state.net_ready.borrow() {
        (StatusCode::OK, "ready")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "warming")
    }
}
```

`/healthz` stays unconditionally `200 OK` — it answers "is the process alive," not "has the swarm bound." Compose `healthcheck` consults `/readyz`.

- [ ] **Step 3: Smoke test**

```rust
#[tokio::test]
async fn readyz_flips_after_watch_set() {
    let (tx, rx) = tokio::sync::watch::channel(false);
    let state = Arc::new(AdminState { net_ready: rx, /* ...fill required fields... */ });
    // hit /readyz via axum::Router::oneshot; expect 503
    // tx.send(true).unwrap();
    // hit /readyz again; expect 200
}
```

- [ ] **Step 4: Commit** (`feat(node): /readyz gated on swarm listen readiness`)

---

### Task 12: Fail-closed startup test (spec §8 negative test)

The spec mandates *"a negative test that disables the swarm and asserts the node refuses to start."* The test must exercise the real `runtime::run()` entry point — not a synthetic helper — so a regression that bypasses the gate is caught.

**Files:**

- Modify: `apps/node/src/runtime.rs` — make `run()` return `Result<()>` reachable from an integration test (most likely already true).
- Add: `apps/node/tests/start_fails_closed_in_live_mode.rs`

- [ ] **Step 1: Add the live-mode startup guard in `runtime.rs`**

Immediately after `spawn_gossip_tasks` returns, before constructing the orchestrator:

```rust
if cfg.node.network_mode == "live" && !args.allow_skeleton_network {
    // Swarm must have at least one listen addr. An empty listen list in
    // `live` mode would silently fall back to a non-listening node.
    if cfg.net.listen.is_empty() {
        anyhow::bail!(
            "network_mode=\"live\" requires at least one [net].listen address \
             (or pass --allow-skeleton-network)"
        );
    }
}
```

This is intentionally minimal — a real "swarm failed to bind" path is already surfaced by `spawn_gossip_tasks` propagating `listen_on` errors. The empty-listen case is what the negative test exercises.

- [ ] **Step 2: Add the negative test** (`apps/node/tests/start_fails_closed_in_live_mode.rs`)

```rust
//! Spec §8: default `devnet` profile must fail closed if the swarm cannot
//! claim a listen socket and `--allow-skeleton-network` was not passed.

use std::path::PathBuf;
use tempfile::tempdir;

fn write(dir: &std::path::Path, rel: &str, body: &str) -> PathBuf {
    let p = dir.join(rel);
    if let Some(parent) = p.parent() { std::fs::create_dir_all(parent).unwrap(); }
    std::fs::write(&p, body).unwrap();
    p
}

#[tokio::test]
async fn live_mode_without_listen_addrs_refuses_to_start() {
    let dir = tempdir().unwrap();
    // Profile is `live` but the listen list is empty: must error.
    write(dir.path(), "profiles/devnet.toml", r#"
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
"#);

    // Call the public runtime entry with allow_skeleton_network=false.
    let result = node::runtime::run_for_test(
        node::runtime::TestArgs {
            config_dir: dir.path().to_path_buf(),
            profile: "devnet".into(),
            allow_skeleton_network: false,
        },
    ).await;

    let err = result.expect_err("must refuse to start in live mode with empty listen");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("network_mode") && msg.contains("listen"),
        "unexpected error: {msg}"
    );
}

#[tokio::test]
async fn allow_skeleton_network_bypasses_the_gate() {
    let dir = tempdir().unwrap();
    write(dir.path(), "profiles/devnet.toml", /* same body as above */);

    // Same config, but the escape hatch is set: should not fail at the gate.
    // (It may still fail later for unrelated reasons; assert the error, if any,
    //  is NOT about the listen-list guard.)
    let result = node::runtime::run_for_test(
        node::runtime::TestArgs {
            config_dir: dir.path().to_path_buf(),
            profile: "devnet".into(),
            allow_skeleton_network: true,
        },
    ).await;

    if let Err(err) = result {
        let msg = format!("{err:#}");
        assert!(!msg.contains("network_mode=\"live\" requires"));
    }
}
```

`run_for_test` is a thin wrapper around `run()` that builds the args struct from the test parameters and immediately requests shutdown after the gate runs (e.g. by passing a pre-cancelled shutdown signal). Add it in `apps/node/src/runtime.rs` behind `#[cfg(any(test, feature = "test-helpers"))]` — keep the surface minimal: `config_dir`, `profile`, `allow_skeleton_network` are enough for these two tests.

- [ ] **Step 3: Run the test**

```bash
cargo test -p node --test start_fails_closed_in_live_mode --locked -- --nocapture
```

Expected: both tests pass.

- [ ] **Step 4: Commit** (`test(node): refuse to start when live mode has no listen addrs`)

---

### Task 13: Docker + Compose + workflows

**Files:**

- Modify: `Dockerfile`
- Modify: `docker-compose.yml`
- Modify: `.github/workflows/docker-smoke.yml`
- Modify: `.github/workflows/ci.yml`
- Optional: add a `node --health-probe` subcommand in `apps/node/src/main.rs`

**Port mapping table (single source of truth, used everywhere below):**

| Service | Container `[net].listen` | Host gossip port | Container admin | Host admin |
|---------|--------------------------|------------------|-----------------|------------|
| node0   | `9000/tcp`               | `9000`           | `9100/tcp`      | `9100`     |
| node1   | `9000/tcp`               | `9001`           | `9100/tcp`      | `9101`     |
| node2   | `9000/tcp`               | `9002`           | `9100/tcp`      | `9102`     |
| node3   | `9000/tcp`               | `9003`           | `9100/tcp`      | `9103`     |

Inside the container every node listens on **`9000/tcp`** (gossip) and **`9100/tcp`** (admin) — identical config, only the host-side port and the identity label vary.

- [ ] **Step 1: Dockerfile**

Use the workspace `rust-toolchain.toml` (1.88) in the builder stage. Multi-stage; non-root runtime user; copy the `config/` tree non-secret subset.

```dockerfile
FROM rust:1.88-bookworm AS builder
WORKDIR /build
COPY . .
RUN cargo build --release -p node --bin node

FROM debian:bookworm-slim
RUN useradd --system --create-home --shell /usr/sbin/nologin node \
 && mkdir -p /data/rocksdb && chown node:node /data/rocksdb
USER node
WORKDIR /home/node
COPY --from=builder --chown=node:node /build/target/release/node /usr/local/bin/node
COPY --from=builder --chown=node:node /build/config /home/node/config
EXPOSE 9000 9100
ENTRYPOINT ["/usr/local/bin/node"]
CMD ["--profile", "devnet", "--config-dir", "/home/node/config"]
```

- [ ] **Step 2: Pick a healthcheck strategy and stick to it**

**Preferred:** add a `node --health-probe` subcommand that performs a local `GET /readyz` against `127.0.0.1:9100` and exits `0` on `200`, non-zero otherwise. Then `HEALTHCHECK` invokes `node --health-probe`. No `curl` needed in the runtime image.

**Acceptable fallback:** install `curl` in the runtime image and use `HEALTHCHECK CMD curl -fsS http://127.0.0.1:9100/readyz`. Document the trade-off in `docker/README.md` if this path is taken.

Pick one in the implementation PR; do not ship both.

- [ ] **Step 3: `docker-compose.yml`**

Four services on the default bridge, hostnames `node0`..`node3`. Bootstrap multiaddrs are the **other three** PeerIDs per node (paste the values from Task 5 step 3).

```yaml
version: "3.9"

x-node-common: &node-common
  build: .
  image: lua-node:dev
  restart: unless-stopped
  healthcheck:
    test: ["CMD", "/usr/local/bin/node", "--health-probe"]
    interval: 5s
    timeout: 2s
    retries: 12

services:
  node0:
    <<: *node-common
    hostname: node0
    environment:
      LUA_DAG_PROFILE: devnet
      LUA_DAG_NODE_IDENTITY_LABEL: node0
      LUA_DAG_BOOTSTRAP_PEERS: "/dns4/node1/tcp/9000/p2p/<PID1>,/dns4/node2/tcp/9000/p2p/<PID2>,/dns4/node3/tcp/9000/p2p/<PID3>"
    ports: ["9000:9000", "9100:9100"]
    volumes: ["./devnet-data/node0:/data/rocksdb"]
  node1:
    <<: *node-common
    hostname: node1
    environment:
      LUA_DAG_PROFILE: devnet
      LUA_DAG_NODE_IDENTITY_LABEL: node1
      LUA_DAG_BOOTSTRAP_PEERS: "/dns4/node0/tcp/9000/p2p/<PID0>,/dns4/node2/tcp/9000/p2p/<PID2>,/dns4/node3/tcp/9000/p2p/<PID3>"
    ports: ["9001:9000", "9101:9100"]
    volumes: ["./devnet-data/node1:/data/rocksdb"]
  node2:
    <<: *node-common
    hostname: node2
    environment:
      LUA_DAG_PROFILE: devnet
      LUA_DAG_NODE_IDENTITY_LABEL: node2
      LUA_DAG_BOOTSTRAP_PEERS: "/dns4/node0/tcp/9000/p2p/<PID0>,/dns4/node1/tcp/9000/p2p/<PID1>,/dns4/node3/tcp/9000/p2p/<PID3>"
    ports: ["9002:9000", "9102:9100"]
    volumes: ["./devnet-data/node2:/data/rocksdb"]
  node3:
    <<: *node-common
    hostname: node3
    environment:
      LUA_DAG_PROFILE: devnet
      LUA_DAG_NODE_IDENTITY_LABEL: node3
      LUA_DAG_BOOTSTRAP_PEERS: "/dns4/node0/tcp/9000/p2p/<PID0>,/dns4/node1/tcp/9000/p2p/<PID1>,/dns4/node2/tcp/9000/p2p/<PID2>"
    ports: ["9003:9000", "9103:9100"]
    volumes: ["./devnet-data/node3:/data/rocksdb"]
```

- [ ] **Step 4: `.github/workflows/ci.yml` — toolchain alignment**

Replace any pinned older toolchain with:

```yaml
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: "1.88"
          components: rustfmt, clippy
```

Apply to **every** job that compiles Rust (fmt, clippy, build, test).

- [ ] **Step 5: `.github/workflows/docker-smoke.yml` — single-replica readiness**

```yaml
- name: Build node image
  run: docker build -t lua-node:ci .

- name: Run single-replica smoke
  run: |
    docker run --rm -d --name lua-node-smoke -p 9100:9100 lua-node:ci
    # Poll /readyz for up to 60s.
    for i in $(seq 1 30); do
      if curl -fsS http://127.0.0.1:9100/readyz; then
        echo "ready"; docker rm -f lua-node-smoke; exit 0
      fi
      sleep 2
    done
    docker logs lua-node-smoke
    docker rm -f lua-node-smoke
    exit 1
```

- [ ] **Step 6: `.github/workflows/docker-smoke.yml` — four-node Compose smoke (spec §8 acceptance)**

This step is **not optional**; spec §8 names the four-node run as the acceptance demo. Keep it on the default path:

```yaml
- name: Four-node Compose smoke
  run: |
    docker compose up -d --build
    # Wait for all four services to report healthy.
    deadline=$(( $(date +%s) + 180 ))
    until [ $(docker compose ps --status=running --services | wc -l) -ge 4 ] \
          && docker compose ps --format json | jq -e 'all(.Health == "healthy")' >/dev/null; do
      if [ $(date +%s) -gt $deadline ]; then
        echo "Timed out waiting for 4-node devnet healthy"
        docker compose logs --no-color
        exit 1
      fi
      sleep 3
    done
    # Sanity: each admin port answers /readyz.
    for port in 9100 9101 9102 9103; do
      curl -fsS http://127.0.0.1:$port/readyz
    done
    docker compose down -v
```

If this step proves flaky in the first PR, the fix is to harden the readiness signal — not to mark the step `continue-on-error`. Multi-node smoke gates the acceptance criterion; demoting it silently undoes the spec.

- [ ] **Step 7: Commit** (`feat(ci+docker): node image, four-node compose smoke, toolchain 1.88`)

---

### Task 14: Documentation

**Files:**

- Modify: `README.md` — short "run devnet" section
- Modify: `docker/README.md` — Phase B completion + healthcheck strategy chosen in Task 13 step 2

- [ ] **Step 1: Root README "run devnet" section**

```bash
# 4-node devnet
docker compose up --build -d

# Each node exposes admin on host port 9100..9103 (see docker-compose.yml).
curl -fsS http://127.0.0.1:9100/readyz
curl -fsS http://127.0.0.1:9101/readyz
curl -fsS http://127.0.0.1:9102/readyz
curl -fsS http://127.0.0.1:9103/readyz
```

Cross-link to the spec (`docs/superpowers/specs/2026-05-15-devnet-prodlike-design.md`) for the full config and identity model.

- [ ] **Step 2: Env whitelist table** (paste the table from Task 4 step 2)

- [ ] **Step 3: `docker/README.md`** — record which healthcheck strategy was picked (built-in `--health-probe` vs `curl`) so future contributors don't reintroduce the other.

- [ ] **Step 4: Commit** (`docs: run-devnet quickstart + env whitelist`)

---

## Self-review (plan vs spec)

| Spec § | Satisfied by Task |
|--------|-------------------|
| §3.1 layered TOML + array-replace merge | Task 3 (`merge_toml` + four unit tests) |
| §3.2 env whitelist + bootstrap replace | Task 4 |
| §3.4 deterministic node identity | Tasks 2, 5, 7 (DST + binary + golden test) |
| §4.1 TCP/Noise/Yamux devnet transport | Task 6 |
| §4.1 live gossipsub swarm replaces skeleton broadcast drop | Tasks 8–10 |
| §5.2 `/dns4` bootstrap multiaddrs + port mapping | Tasks 5, 13 |
| §5 admin `/readyz` gated on listen readiness | Tasks 9 (watch), 11 (axum route) |
| §6 CI Rust 1.88 across all jobs | Task 13 |
| §8 negative test: live mode fails closed | Task 12 |
| §8 acceptance: 4-node Compose smoke | Task 13 step 6 |
| §9 phase 3 (a): delete `lua_dag_smoke` | Task 1 |
| §5.3 root README "run devnet" | Task 14 |

**Placeholder scan:** the only intentional placeholders are the four `<PASTE PEER ID HERE>` literals in `devnet_identity_golden.rs` and the matching `<PID*>` slots in `docker-compose.yml`. Both are filled in Task 5 step 3 from `print_devnet_peer_ids` output — must be resolved before PR merge (the golden test will fail otherwise).

---

## Verification commands (pre-merge checklist)

```bash
# Rust
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

# Image build
docker build -t lua-node:local .

# Four-node devnet smoke (run in foreground briefly, then tear down).
docker compose up -d --build
for port in 9100 9101 9102 9103; do
  for i in $(seq 1 30); do
    if curl -fsS http://127.0.0.1:$port/readyz; then break; fi
    sleep 2
  done
done
docker compose down -v
```

All commands must succeed before the PR is mergeable.
