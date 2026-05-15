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
| Array merge `[net].bootstrap`, etc. | **Within one profile file**, arrays behave as serde defines. **`LUA_DAG_BOOTSTRAP_PEERS`** performs a **whole-list replace** post-parse. Multi-profile table merge (**spec §3.1**) deferred to testnet layering work (**Task 3 conformance note**). |
| `LUA_DAG_BOOTSTRAP_PEERS` | **Replace** merged list (comma-separated). Empty string ⇒ treat as absent (keep file-derived list). |
| Dev identity | **`[node.identity]`** → `kind = "devnet_seed"` + `label = "...")` derives **Ed25519** key via **`crypto::hash::blake3_with_dst`** (new DST `DEVNET_PEER_IDENTITY`) feeding **`libp2p::identity::Keypair::ed25519_from_bytes(&mut [...])`** (clamp/retry documented in Task 7 if `Err` ever occurs — deterministic labels must be chosen once in tests). Compose sets **`LUA_DAG_NODE_IDENTITY_LABEL`** per service to **`node0`…`node3`** so **one checked-in profile** suffices. |
| Transport | **`build_transport_tcp_only`** — no QUIC mux in swarm for **devnet** profile (still may depend on QUIC-free libp2p build flags if needed later). Existing `build_transport` remains for callers that still want QUIC+TCP until deprecated. |
| Bootstrap wire | **`/dns4/<hostname>/tcp/<port>/noise/p2p/<PeerID>`** is WRONG chain — Noise is not dial component. Correct: **`/dns4/node1/tcp/9000/p2p/<PeerID>`** (**Noise negotiated after TCP dial** via transport — match libp2p multiaddr semantics). |
| Integration test preference | **`127.0.0.1` loopback TCP** dual-process or dual-task pattern inside **`crates/net/tests/`** publishing **dummy `MicroQc`** payloads (or smallest `BroadcastMicroQc`-compatible struct) subscribed on topic **`lua-dag/v1/micro-qc`** rather than QUIC in GA CI. |

---

## File structure map

```
crates/crypto/src/hash.rs                             # DST `DEVNET_PEER_IDENTITY`
crates/net/src/transport.rs                            # ADD `build_transport_tcp_only`
crates/net/src/identity.rs                             # OPTIONAL re-export helper OR new `deterministic.rs`
crates/net/src/deterministic_key.rs                    # NEW: `pub fn devnet_keypair_from_label(...) -> Result<Keypair>`
crates/net/src/gossip_wire.rs                          # NEW: action→topic payload; topic+bytes→event (total fn)
crates/net/src/swarm_runner.rs                         # NEW: builds Swarm, drive loop futures::select swarm + rx actions
crates/net/src/lib.rs                                   # expose new modules / factory
crates/net/Cargo.toml                                   # deps if needed (minimal)
apps/node/src/args.rs                                   # --profile, --config-dir, --allow-skeleton-network, identity label overrides
apps/node/src/config.rs                                 # rich load: merge consensus + NetConfig + StorageConfig + network_mode
apps/node/src/config_layers.rs                           # NEW: profile TOML types + filesystem paths (no speculative merge stubs)
apps/node/src/runtime.rs                                # spawn swarm + wire channels; readiness gate
apps/node/src/orchestrator.rs                           # outbound path: enqueue to swarm inbound OR keep bridge translating to publish handle
apps/node/src/observability/health.rs                   # inject readiness: live requires swarm.connected_peers()>0 OR explicit flag WAIT_FOR_PEER
apps/node/src/main.rs                                   # optionally subcommands (see Task 13)
config/default.toml                                     # KEEP as layer-1 consensus base (tables only as today)
config/profiles/devnet.toml                             # NEW: `[net]`, `[node]`, overrides
docker-compose.yml                                       # EDIT: commands, ports=9000, env, bootstrap, healthcmd
Dockerfile                                               # EDIT: cargo build `-p node`, COPY config subtree
docker/README.md
README.md
.gitignore                                               # VERIFY `config/secrets/` if added
docs/superpowers/specs/2026-05-14-docker-node-dev-design.md  # OPTIONAL cross-note "superseded for runtime by ..."
.github/workflows/ci.yml                                 # toolchain 1.88
.github/workflows/docker-smoke.yml                       # node image curl /readyz
Cargo.toml                                                 # REMOVE tools/lua_dag_smoke from members (delete crate)
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
git commit -m "build: drop lua-dag_smoke workspace crate for prod-like devnet"
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

- [ ] **Step 2: Add regression test proving stability**

Still in `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn devnet_peer_identity_dst_is_unique() {
        use std::collections::HashSet;
        let ids = vec![
            dst::DEVNET_PEER_IDENTITY,
            dst::CONTENT_HASH,
        ];
        let mut set = HashSet::new();
        for &i in &ids {
            assert!(set.insert(i));
        }
    }
```

- [ ] **Step 3: Run scoped test**

Run:

```bash
cargo test -p crypto hash::tests::devnet_peer_identity_dst_is_unique --locked -- --nocapture
```

Expected: **PASS**

- [ ] **Step 4: Commit**

---

### Task 3: Profile TOML types + paths (**avoid `serde` clashes with consensus `storage`**)

**Rationale:** `config/default.toml` stays a **`consensus::Config`** document. Reusing **`[storage]`** for RocksDB collides with consensus GC tables. Load **network operational data** from **`config/profiles/<profile>.toml`** only.

**Files:**

- Create: `apps/node/src/config_layers.rs`

- [ ] **Step 1: Add `apps/node/src/config_layers.rs`**

```rust
//! Profile file schema (`spec §3`): network + ops; consensus stays in `default.toml`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

use net::NetConfig;
use storage::StorageConfig;

/// Root of `config/profiles/<profile>.toml`.
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

pub fn load_profile_file(path: &Path) -> Result<ProfileFile> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read profile {}", path.display()))?;
    toml::from_str(&raw).with_context(|| format!("parse profile {}", path.display()))
}
```

Wire `mod config_layers;` from the module tree that already declares `mod config;` in **`apps/node`**.

In **`NodeConfig::load`**, validate **`node.identity.kind == "devnet_seed"`** unless **`--allow-skeleton-network`**.

- [ ] **Step 2: Add `#[cfg(test)] mod tests` to `config_layers.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parses_fixture_profile_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("devnet.toml");
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(
            br#"
[node]
network_mode = "live"

[node.identity]
kind = "devnet_seed"
label = "fixture-node"

[net]
listen = ["/ip4/0.0.0.0/tcp/9000"]
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
path = "/tmp/db"
create_if_missing = true
max_total_wal_size_mb = 128
"#,
        )
        .unwrap();
        let parsed = load_profile_file(&p).unwrap();
        assert_eq!(parsed.node.identity.label, "fixture-node");
        assert_eq!(
            parsed.rocksdb.as_ref().unwrap().max_total_wal_size_mb,
            128
        );
    }
}
```

Add **`tempfile`** to **`apps/node`** **dev-dependencies** if not already listed.

- [ ] **Step 3: Run tests**

```bash
cargo test -p node config_layers::tests --locked -- --nocapture
```

Expected: **PASS**.

- [ ] **Step 4: Commit** (`feat(node): add typed profile loader for devnet`)

**Conformance note:** **§3.1 multi-file merge is deferred.** **Consensus `default.toml` + single `profiles/<name>.toml` + optional `LUA_DAG_BOOTSTRAP_PEERS` replace** fulfills devnet Compose until **`testnet` profile layering** arrives.

---

### Task 4: Wire **`NodeConfig::load`** → layered paths + env whitelist

**Files:**

- Modify: `apps/node/src/config.rs`
- Modify: `apps/node/src/args.rs`

**Args additions (`apps/node/src/args.rs`) — illustrative patch:**

```rust
    /// Deployment profile selecting `profiles/<profile>.toml` under config dir.
    #[arg(long, default_value = "devnet")]
    pub profile: String,

    /// Directory containing `default.toml`, `profiles/`, optional `local.toml`.
    #[arg(long, default_value = "config")]
    pub config_dir: PathBuf,

    /// Dev escape hatch — allows skeleton dropping network broadcasts.
    #[arg(long, hide = true)]
    pub allow_skeleton_network: bool,

    /// Per-container identity label (Compose). Overrides `[node.identity].label`.
    #[arg(long)]
    pub identity_label: Option<String>,
```

**Env ingestion inside `NodeConfig::load`** (pseudocode for location `apps/node/src/config.rs`) — MUST be coded fully by implementer:

```rust
const ENV_PROFILE: &str = "LUA_DAG_PROFILE";
const ENV_CFG_DIR: &str = "LUA_DAG_CONFIG_DIR";
const ENV_STORAGE_PATH: &str = "STORAGE_PATH";
const ENV_BOOTSTRAP: &str = "LUA_DAG_BOOTSTRAP_PEERS";
const ENV_IDENTITY_LABEL: &str = "LUA_DAG_NODE_IDENTITY_LABEL";

fn getenv_trim(key: &str) -> Option<String> {
    std::env::var(key).ok().map(|v| v.trim().to_owned()).filter(|s| !s.is_empty())
}
```

Resolution order precedence for **profiles path**:

1. **`Args.config_dir`** if passed explicitly **OR** **`LUA_DAG_CONFIG_DIR`** if set (**env wins over CLI default** ONLY if explicitly specified in implementation plan refinement — authoritative choice: **`CLI overrides env default but env LUA_DAG_CONFIG_DIR overrides `"config"` string default`** when CLI default used — implement **`clap`** `env(..)` helpers:

```rust
    #[arg(long, default_value = "config", env = "LUA_DAG_CONFIG_DIR")]
    pub config_dir: PathBuf,
```

Similarly:

```rust
    #[arg(long, default_value = "devnet", env = "LUA_DAG_PROFILE")]
    pub profile: String,

    #[arg(long, env = "LUA_DAG_NODE_IDENTITY_LABEL")]
    pub identity_label: Option<String>,
```

**`STORAGE_PATH`:** **`clap`** `env = "STORAGE_PATH"` tie to `--data-dir` resolution.

**Bootstrap replace:** AFTER merge, **`if let Some(s) = getenv_trim(ENV_BOOTSTRAP)`** → split by `','`, trim entries → **`net.bootstrap = parsed`**.

**Fail closed:**

```rust
if cfg.node.network_mode == "live"
    && !args.allow_skeleton_network
    && swarm_not_configured_placeholder
{
    // filled after swarm exists
}
```

**Keep separate until Task 11.** At Task 11, **error** (`anyhow::bail!`) if `network_mode=live` AND `allow_skeleton_network` false AND swarm builder returns error opening listeners.

---

### Task 5: Checked-in **`config/profiles/devnet.toml`** (+ secrets doc touch-up)

**Files:**

- Create: `config/profiles/devnet.toml`
- Optional: tweak `config/README.md` if it mentions only `local.toml`.

**Starter `devnet.toml`** (preferred **`bootstrap`** empty — Compose injects **`LUA_DAG_BOOTSTRAP_PEERS`** so PeerIDs rotate with deterministic labels cleanly):

```toml
[node]
network_mode = "live"

[node.identity]
kind = "devnet_seed"
# Compose MUST set LUA_DAG_NODE_IDENTITY_LABEL; bare-metal runners may keep `label = "node0"`.
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

**PeerID discovery:** extend Task 7 test module with asserts on **expected `PeerId` strings** per label **`node0`…`node3`** (golden strings committed once computed). Embed those literals into Compose **`LUA_DAG_BOOTSTRAP_PEERS`** (comma-separated **`/dns4/...`** multiaddrs).

**Compose env pattern:**

```yaml
environment:
  LUA_DAG_PROFILE: devnet
  LUA_DAG_NODE_IDENTITY_LABEL: node0
  LUA_DAG_BOOTSTRAP_PEERS: "/dns4/node1/tcp/9000/p2p/<PeerId1>,/dns4/node2/tcp/9000/p2p/<PeerId2>,..."
```

---

### Task 6: **`build_transport_tcp_only`**

**Files:**

- Modify: `crates/net/src/transport.rs`

Append:

```rust
/// QUIC-free transport for Compose + CI (**spec §4.1** TCP devnet baseline).
pub fn build_transport_tcp_only(keypair: &Keypair) -> Result<Boxed<(PeerId, StreamMuxerBox)>> {
    let tcp_transport = tcp::tokio::Transport::new(tcp::Config::new().nodelay(true));
    let noise =
        noise::Config::new(keypair).map_err(|e| Error::Transport(e.to_string()))?;
    let yamux = yamux::Config::default();
    Ok(tcp_transport
        .upgrade(upgrade::Version::V1Lazy)
        .authenticate(noise)
        .multiplex(yamux)
        .boxed())
}
```

Expose via `crate::transport::`.

- [ ] **Step: Run compile**

```bash
cargo check -p net --locked
```

---

### Task 7: **`devnet_keypair_from_label`**

**Files:**

- Create: `crates/net/src/deterministic_key.rs`
- Modify: `crates/net/src/lib.rs`

```rust
//! Deterministic (**dev-only**) libp2p keys from textual labels (**spec §3.4**).

use crypto::hash::{blake3_with_dst, dst};
use libp2p::identity::Keypair;
use types::crypto_types::Hash32;

use crate::{Error, Result};

/// Derives a libp2p **Ed25519** key deterministically — **never** use for prod profiles.
///
/// Collision resistance comes from BLAKE3 + fixed DST separation.
#[must_use = "consume the Keypair in Swarm identity setup"]
pub fn devnet_keypair_from_label(label: &str) -> Result<Keypair> {
    let Hash32(mut bytes) =
        blake3_with_dst(dst::DEVNET_PEER_IDENTITY, label.as_bytes());
    let kp = Keypair::ed25519_from_bytes(&mut bytes).map_err(|_| {
        Error::Transport(format!(
            "ed25519 key material rejected by libp2p for label `{label}`"
        ))
    })?;
    Ok(kp)
}
```

- [ ] **Test:** same label ⇒ same **`PeerId`**

```rust
#[test]
fn devnet_identity_stable() {
    let a = devnet_keypair_from_label("node0").unwrap();
    let b = devnet_keypair_from_label("node0").unwrap();
    assert_eq!(a.public().to_peer_id(), b.public().to_peer_id());
}
```

---

### Task 8: **`gossip_wire`** — topic payload mapping

**Files:**

- Create: `crates/net/src/gossip_wire.rs`

**Functions (signatures authoritative):**

```rust
use consensus::{action::Action, event::Event};
use crate::error::Result;

pub fn outbound_broadcast(action: &Action) -> Result<Option<(crate::gossip::Topic, Vec<u8>)>>;

pub fn inbound_message(topic_str: &str, data: &[u8]) -> Result<Option<Event>>;
```

**Outbound rules (`match action`):**

- `BroadcastMicroQc(ref m)` → `Topic::MicroQc`, `encode_action_payload(m)?`
- `BroadcastMacroProposal(m)` → `Topic::MacroProposal`, `encode_action_payload(m)?`
- `BroadcastBlsPartial(p)` → `Topic::BlsPartial(p.subnet)`, `encode_action_payload(p)?`
- `BroadcastSubnetAggregate(a)` → `Topic::SubnetAggregate`, `encode_action_payload(a)?`
- `BroadcastMacroQc(q)` → `Topic::MacroQc`, `encode_action_payload(q)?`
- **`EmitSlashEvidence { evidence }`** → **`Topic::SlashEvidence`**, **`encode_action_payload(evidence)?`**
- **`Action::PersistMacroQc`** / **`ScheduleTimer`** / **`CancelTimer`** / **`UpdateBlobStatus`** → **`Ok(None)`** (**host-local**, not gossip)

**Outbound errors:** **`Err(...)`** if `encode_action_payload` fails (**never silently drop failing serialization**).

```rust
use consensus::action::Action;
use crate::gossip::{Topic, codec::encode_action_payload};

/// Non-broadcast variants return [`None`].
pub fn outbound_broadcast(action: &Action) -> crate::Result<Option<(Topic, Vec<u8>)>> {
    Ok(Some(match action {
        Action::BroadcastMicroQc(m) => (Topic::MicroQc, encode_action_payload(m)?),
        Action::BroadcastMacroProposal(m) => (Topic::MacroProposal, encode_action_payload(m)?),
        Action::BroadcastBlsPartial(p) => (Topic::BlsPartial(p.subnet), encode_action_payload(p)?),
        Action::BroadcastSubnetAggregate(a) => (Topic::SubnetAggregate, encode_action_payload(a)?),
        Action::BroadcastMacroQc(q) => (Topic::MacroQc, encode_action_payload(q)?),
        Action::EmitSlashEvidence { evidence, .. } => (Topic::SlashEvidence, encode_action_payload(evidence)?),
        Action::PersistMacroQc(_) | Action::ScheduleTimer { .. } | Action::CancelTimer(_) | Action::UpdateBlobStatus { .. } => {
            return Ok(None);
        }
    }))
}
```

**Inbound rules:**

- Match `topic_str == Topic::<Variant>.ident().to_string()` (**compare** normalized string)
- **`CertifiedVertex`**: deserialize `CertifiedVertex` → `Event::CertifiedVertexReceived`
- etc.

**Subnet topics:** strip prefix **`lua-dag/v1/bls-partial/`** parse `u32` → **`SubnetId`**.

- [ ] **Unit tests**: round-trip **`TimerId`** NOT VALID — replace with **`MicroQc`** minimal fabricated struct respecting `MicroQc`'s serde/borsh needs — use **`consensus`** test factories if exist.

---

### Task 9: **`swarm_runner` — swarm build + poll loop**

**Files:**

- Create: `crates/net/src/swarm_runner.rs`

**Public API:**

```rust
pub struct GossipSpawn {
    pub events_tx: tokio::sync::mpsc::Sender<consensus::event::Event>,
    pub ready: tokio::sync::watch::Receiver<bool>,
}

pub async fn spawn_gossip_tasks(
    keypair: libp2p::identity::Keypair,
    net_cfg: NetConfig,
    actions_rx: tokio::sync::mpsc::Receiver<consensus::action::Action>,
) -> anyhow::Result<GossipSpawn>;
```

**Implementation outline (implementer expands to compilable Rust):**

1. Build **`tcp_transport`** with `crate::transport::build_transport_tcp_only(&keypair)`.
2. Construct **`Gossipsub::new(MessageAuthenticity::Signed(...), gossipsub_cfg)`** with heartbeat from `NetConfig`.
3. **Subscribe** iterable of static topics (base **non-subnet-specific** enumeration first + optional dynamic subnet placeholders **IF** MVP requires Mode A immediate — MVP may omit subnet-specific listens until aggregator exists — minimal devnet MAY subscribe wildcard impossible — **minimal**: subscribe **`MicroQc`**, **`MacroProposal`**, **`MacroQc`** only initially **IF** narrowing needed — **SPEC requires no silent drops** on published actions ⇒ subscribe **FULL enum topics without subnet subsets** PLUS **SubnetId(0..K)** small constant `K`: document `K = 256` brute membership OR postpone `BroadcastBlsPartial` until subnets active — authoritative MVP: **subscribe EVERY `Topic::*` excluding dynamic subnet** using **subnet 0** topic only initially + document limitation.

**Narrows clarification:** MVP integration test publishes **ONLY `MicroQc`**; subscribe **`MicroQc`** + **`MacroQc`** for smoke.

4. `Swarm::listen_on` parsed multiaddrs sequentially.
5. `for addr in bootstrap { swarm.dial(addr.parse()?) }`.
6. `tokio::spawn` swarm loop **`loop { swarm.select_next_some().await; ... }`** OR `futures::StreamExt`-style **`Swarm.poll`**.

Outbound channel branch:

```rust
Some(action) = actions_rx.recv() => {
    if let Some((topic,payload)) = gossip_wire::outbound_broadcast(&action)? {
       swarm.behaviour_mut().gossipsub.publish(topic.ident(),payload)?;
    }
}
```

Gossip inbound:

```rust
SwarmEvent::Behaviour(GossipsubEvent::Message { message, .. }) => {
   if let Some(ev) = gossip_wire::inbound_message(&message.topic,...){
      let _ = events_tx.send(ev).await;
   }
}
```

Expose readiness **true** once **either**: at least **1** gossip peer OR **explicit** **`NetworkReadyPolicy::ImmediatelyAfterListen`** flagged for CI — authoritative: **`READY after first successful listen + identify peer count`** using **`Gossipsub`** mesh **peer count**.

Set watch channel true when **`swarm.connected_peers().len()`** reports **>**0 **AND** **`bootstrap` nonempty** optional wait — Compose may require **immediate ready** bootstrapless — unify: **`ready`** true after **listening sockets bound**.

**Minimal:** **`ready`** flips **`true`** when **all listen addrs succeeded** (bootstrap optional). Document in code comment.

Admin `/readyz` consults **`ready` watch**.

---

### Task 10: Integrate swarm into **`apps/node` runtime**

**Files:**

- Modify: `apps/node/src/runtime.rs`
- Modify: `apps/node/src/orchestrator.rs`
- Possibly remove unused `_bridge_handle` misuse

**Orchestrator change:** outbound `Bridge::translate_action` **delegates** OR **Orchestrator** receives second channel — simplest: **`Bridge`** extended with **`Option<BroadcastSink>`**:

**AUTHORITATIVE:** Delete reliance on **`Bridge::translate_action`** inside orchestrator loop for broadcast — replace with **`net::enqueue_action(actions_tx_clone, action)`** (`Result` surfaced).

Maintain **`Translator`** only for **`ScheduleTimer`/storage** classifications OR inline `match` splitting **timer vs gossip**.

**Orchestrator loop sketch:**

```rust
for action in actions {
    match &action {
        a if gossip_wire_classify(a).is_broadcast() => { net_tx.try_send((*a).clone())?; /* or `.await send` */}
        other => Bridge::translate_action(other)? ,
    }
}
```

Prefer **single **`Action` processing function** implemented in **`apps/node/src/net_dispatch.rs`**.

Spawn ordering `runtime.rs`:

1. Channels `events_tx`/`events_rx` already exist — **reuse**.
2. `spawn_gossip_tasks(keypair,...)` awaits until listen ok.
3. Pass **`Arc<watchSender>`** readiness into **`serve_admin`** with new param.

---

### Task 11: **`serve_admin`** readiness coupling

**Files:**

- Modify: `apps/node/src/observability/health.rs`

Add `Arc<AtomicBool>` or `watch::Receiver<bool>` to `AdminState`.

**Route `/readyz`:**

```rust
async fn readyz(State(state): State<Arc<AdminState>>) -> impl IntoResponse {
    if state.net_ready.borrow().cloned() /* or Atomic load */ {
        (StatusCode::OK, "ready")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "warming")
    }
}
```

`/healthz` stays **always 200**.

---

### Task 12: Fail-closed startup tests

**Files:**

- Add: `apps/node/tests/start_requires_network_skeleton_flag.rs`

**Test approach:** Spawn `tokio::process::Command` running **`cargo run --bin node --`** with ephemeral config path — heavy.

**Prefer:** refactor **`NodeConfig::validate_network`** pure fn called from **`runtime::run()`** asserting:

```rust
pub fn enforce_network_modes(cfg:&NodeCaps,allow_skeleton:bool)->anyhow::Result<()>{
 if cfg.network_mode!="live"{return Ok(());}  
 if cfg.allow_skeleton_flag_unwired { /* test inject */ }
 Ok(())
}
```

**Spec negative test pragmatic delivery:** **`#[tokio::test]`** invoking **`enforce_network_modes`** expecting error when **`network_mode=live`** + internal flag says **transport disabled** synthetic.

Adapt language to finalized structure.

---

### Task 13: Docker + Compose + workflows

**Files:**

- Modify: `Dockerfile`
- Modify: `docker-compose.yml`
- Modify: `.github/workflows/docker-smoke.yml`
- Modify: `.github/workflows/ci.yml`

**Dockerfile authoritative lines:**

```dockerfile
COPY config ./config
RUN cargo build --release -p node --bin node
COPY --from=builder /build/target/release/node /usr/local/bin/node
ENTRYPOINT ["/usr/local/bin/node"]
```

(Add args default `--profile devnet`.)

Non-root **`USER node`**: **`COPY --chown=node`** config subtree.

**.github/workflows/ci.yml toolchain bump:**

Replace `toolchain: 1.85` with:

```yaml
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: "1.88"
          components: rustfmt, clippy
```

**(fmt job too).**

**`docker-smoke.yml`:**

```yaml
docker run ... -p 9100:9100 lua-node:ci --admin-listen 0.0.0.0:9100 &
sleep ...
curl -fsS localhost:9100/readyz | grep ready
```

**Multi-node Compose smoke** OPTIONAL follow-up flagged `continue-on-error: true` ONLY if flaky — preferably **dual-container** deterministic wait loop.

---

### Task 14: Documentation

**Files:**

- Modify: `README.md`
- Modify: `docker/README.md`

Minimal sections:

Run:

```bash
docker compose up --build
curl http://localhost:39100/readyz  # illustrate offset formula
```

Document env whitelist table.

---

## Self-review (plan vs spec)

| Spec § | Satisfied by Task |
|--------|-------------------|
| Goals: P2P not skeleton default | Tasks 8–11 |
| Layered `[net]` + arrays replace | Tasks 3–5 |
| Env whitelist | Task 4 |
| Identity deterministic §3.4 | Tasks 7–8 snapshots |
| TCP devnet §4.1 | Task 6 |
| `/dns4` bootstrap §5.2 | Task 5 / compose |
| Health `/readyz` §5 | Task 11 |
| CI Rust 1.88 §6 | Task 13 |
| Remove smoke default §9 | Task 1 |
| Negative fail-closed §8 | Task 12 |
| Root README §5.3 | Task 14 |

**Placeholder scan:** Remove any remaining `REPLACE_ME`/unreachable merges before merging PR — Tasks 3 & 8 explicitly call out reconciliation duties.

---

## Verification commands (pre-merge checklist)

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
docker build -t lua-node:local .
docker compose up --abort-on-container-exit
```

---

Plan complete và đã lưu tại **`docs/superpowers/plans/2026-05-15-devnet-prodlike.md`**.

**Hai cách triển khai:**

**1. Subagent-Driven (khuyến nghị)** — mỗi task một agent con, review giữa các task.  
**2. Inline Execution** — làm tuần tự trong một session (`executing-plans`), có checkpoint để duyệt.

Bạn muốn đi **(1)** hay **(2)**?
