# Design: Prod-like devnet (4 nodes, real P2P, unified config)

**Date:** 2026-05-15  
**Status:** Approved for specification (brainstorm option **C**)  
**Audience:** Agents and contributors shipping a credible **devnet** that exercises the **same configuration path** intended for **testnet/prod**, starting with **N = 4**.

**Relations:** Extends **`2026-05-14-docker-node-dev-design.md`** (Docker Phase B checklist becomes part of this rollout). Does not replace **`sim`** (determinism stays in **`apps/sim`**).

---

## 1. Goals

- Run **four** validator processes (local or Compose) wired to **`apps/node`**, **`crates/consensus`**, **`crates/storage`** (RocksDB), and **`crates/net`** with a **live libp2p stack** — no reliance on **`Bridge::translate_action`** dropping broadcast actions on the primary devnet path.
- Configure the node through a **single, layered file model + explicit runtime profile**, not ad-hoc hard-coded Rust defaults that diverge between “Docker dev” and “how we intend to operate later.”
- Package a **Dockerfile** building **`node`** and a **Compose** topology that publishes ports consistent with **`[net].listen`** in the **devnet profile** (one source of truth; no silent mismatch such as scaffold `40000` vs `9000` without documentation).
- Expose operational signals suitable for Compose/CI: **HTTP admin** readiness (existing surface) drives **healthcheck** where applicable.
- Align **GitHub Actions** toolchain with **`rust-toolchain.toml`** (**1.88** today) across fmt/clippy/build/test jobs.
- Extend **Docker CI** smoke from “Rocks only” toward **`node` + readiness**, once inbound/outbound networking is deterministic enough to avoid flaky pipelines.

---

## 2. Non-goals

- Full **production** hardening (rate limits, distroless audits, HA secrets backends, exhaustive observability stacks).
- **Mainnet** launch automation or turnkey **validator ceremony** tooling; genesis/key material remains **documentation + `.example`** only — real secrets stay **gitignored** or injected via mounts.
- **Bit-identical** cross-host reproducibility beyond what `sim` guarantees.
- Mandatory **Kubernetes** manifests in this spec (Compose + documented env contract is sufficient; manifests may follow the same profile layout later).

---

## 3. Configuration model (prod-like)

### 3.1 Layered TOML

Fixed merge order (**last wins** per key):

1. **Base** shared parameters (today’s analogue: consensus table from **`config/default.toml`** — implementation may physically split into **`config/base/*.toml`** if that reduces duplication; the spec requires **one documented tree**).
2. **Profile** file for the deployment class, e.g. **`config/profiles/devnet.toml`** (later **`testnet.toml`**, **`prod.toml`**).
3. **Optional local override**, e.g. **`config/local.toml`**, **gitignored**, for developer machine tweaks.
4. **Optional override path** passed on CLI (`--override-config` today keeps working; semantics stay “merged on top of profile stack”).

**Merge semantics:**
- **Tables** merge field-by-field (later layer’s keys overwrite earlier layer’s keys).
- **Arrays** (e.g. **`[net].bootstrap`**, any **`[[peer]]`** array-of-tables) are **replaced wholesale** by the later layer. There is **no item-level merge** for arrays — this avoids ambiguity around ordering and identity-based dedup. Implementations MUST NOT silently concatenate.

**Requirement:** **`[net]`** (listen, bootstrap, gossip, peers) MUST be deserialized from these layers — **`NodeConfig` MUST NOT silently ignore `[net]` and fall back only to `NetConfig::devnet_default()`** after this work completes. Rust may keep `devnet_default()` as **fallback for tests only**, not as the hidden runtime path for Compose.

### 3.2 Runtime selection

- **CLI:** `--profile <name>` selects **`config/profiles/<name>.toml`** (or an equivalent explicit path flag such as **`--profile-config`** — implementation picks one pairing and documents it).
- **Environment:** A **whitelist** mirrors prod/testnet injection (Compose/K8s). Recommended names for the implementation plan (**exact spelling is part of implementation**):
  - **`LUA_DAG_PROFILE`** — profile name (e.g. `devnet`).
  - **`LUA_DAG_CONFIG_DIR`** — root directory for base + profiles if not using working-directory-relative paths.
  - **`STORAGE_PATH`** — RocksDB root (aligns with existing smoke tooling; must match **`StorageConfig.path`** wiring in `node`).
  - **`LUA_DAG_BOOTSTRAP_PEERS`** — optional; comma- or space-separated multiaddrs.

**Bootstrap env precedence (binding decision required in the plan):** The implementation plan MUST pick exactly one rule and document it before any code lands — **(a) replace** the merged-config `[net].bootstrap` array wholesale, or **(b) append** to it with de-duplication by multiaddr. **Recommended:** **(a) replace**, to keep behavior aligned with the array-merge rule in §3.1 and to keep the profile file as the primary source of truth.

**Rule:** No “every environment variable maps to every TOML field.” Only whitelisted variables are read; everything else comes from files.

### 3.3 Secrets and keys

- Never commit real keys. Provide **`.example`** files and a short **secrets** subsection in docs.
- Compose uses **bind mounts** or **Docker secrets** (optional later); devnet **C** requires at least **documented mount points** and **non-root** runtime user in the image (already in prior Docker spec).

### 3.4 Node identity (libp2p keypair)

The four-node devnet requires **deterministic peer IDs** so that the profile’s **`[net].bootstrap`** entries can encode `/p2p/<PeerID>` at write time and remain valid across restarts and clean clones. Three options; the implementation plan MUST pick exactly one for the **`devnet`** profile and document it:

1. **Deterministic seed in profile** (**recommended for devnet**): the **`devnet`** profile carries a per-node **`[node.identity]`** block (e.g. `seed = "node0"`) and **`apps/node`** derives an Ed25519 keypair from that seed at boot. Keys never touch disk; peer IDs are reproducible from the profile alone.
2. **Pre-generated keypair files** mounted at a documented path; profile references the path. Adds a setup step but mirrors how testnet/prod will work.
3. **First-boot generation** with peer ID written to a known file the operator copies into the profile. Rejected for devnet (defeats the “clean clone” acceptance criterion in §8), acceptable later for testnet/prod.

Whatever the choice, the **`devnet`** profile and the Compose bootstrap multiaddrs MUST be **self-consistent at commit time** — no “run once, then edit the profile” bootstrap. For **testnet/prod**, option (2) is the expected path; the `devnet` choice MUST NOT leak deterministic seeds into non-devnet profiles.

---

## 4. Runtime architecture (`apps/node` + `crates/net`)

### 4.1 Swarm / driver ownership

Introduce a clear owner (module or task) for the **libp2p swarm** inside **`apps/node`**:

- Binds **`NetConfig.listen`**.
- Dials **`NetConfig.bootstrap`** on startup.
- Subscribes to gossip topics required for the current protocol surface (implementation enumerates topics in the plan).
- Decodes inbound payloads to **`consensus::Event`** and sends on the existing **`events_tx`** path.
- Consumes **`Action`** from **`Bridge`** / outbound channel and performs **real publish** (and RPC when required by the protocol), using **`net::gossip::codec`** and related modules — **no silent drop** for broadcast-class actions on the devnet default code path.

**Transport (devnet):** the **`devnet`** profile MUST declare the transport explicitly in **`[net].listen`** multiaddrs (e.g. `/ip4/0.0.0.0/tcp/9000` or `/ip4/0.0.0.0/udp/9000/quic-v1`). **Recommended:** **TCP + Noise + Yamux** for devnet and CI smoke — avoids UDP/QUIC NAT and firewall variance that bites Compose-on-CI runners. QUIC may be added behind a separate profile once TCP is green. The transport choice MUST be the same across all four nodes in a given devnet run; mixed-transport bootstraps are out of scope.

### 4.2 Orchestrator

**`Orchestrator`** remains the consensus driver: it receives **`Event`**, runs **`StateMachine::step`**, dispatches **`Action`**. Network I/O lives in the swarm/driver; **timers stay host-local** (existing `TokioClock` / timer actions).

### 4.3 Skeleton bridge

**`Bridge::translate_action`** may remain for **unit tests** or a **no-network** mode if explicitly selected by config — but the **default profile `devnet`** MUST enable the real network path. Any “skeleton only” mode MUST be **opt-in** (flag or profile) so it cannot masquerade as a complete devnet.

---

## 5. Docker and Compose

### 5.1 Dockerfile

- **Builder:** Match workspace **`rust-version`** (**1.88**); same native deps as today for RocksDB.
- **Artifact:** `cargo build --release -p node` (binary name as defined in **`apps/node`**).
- **Runtime:** Non-root user; copy **`node`** binary; copy **minimal `config/` tree** (base + profiles) into the image **OR** document that the image expects a **config volume mount** — pick one in implementation; **recommended:** copy **non-secret** profile files into the image for CI reproducibility, mount **local + secrets** for developers.

### 5.2 `docker-compose.yml`

- Services **`node0` … `node3`** on the default bridge; **hostnames** match peer DNS.
- **Data:** bind mounts under **`./devnet-data/<id>/`** (or equivalent), **`STORAGE_PATH=/data/rocksdb`** inside the container.
- **Ports:** published host ports MUST match the **devnet profile** **`[net].listen`** ports (stride/offset pattern allowed; document the mapping table once).
- **Bootstrap:** internal multiaddrs use **`/dns4/<service>/tcp/<port>/p2p/<PeerID>`** (or the QUIC equivalent `/dns4/<service>/udp/<port>/quic-v1/p2p/<PeerID>`) — **not** `/ip4/<service>/...` (libp2p `/ip4/` requires a literal IP and rejects DNS labels) and **not** `127.0.0.1` (loopback does not cross containers). `<PeerID>` MUST come from the deterministic identity scheme chosen in [§3.4](#34-node-identity-libp2p-keypair).
- **Healthcheck:** HTTP GET against admin **readiness** endpoint (exact path as implemented under **`apps/node/src/observability/health.rs`**). **Prefer a built-in probe** (e.g. a `node --health-probe` subcommand or a tiny statically linked tool) over installing **`curl`**, which inflates image size and surface area. Adding `curl` is acceptable as a documented, time-boxed trade-off **only if** the built-in option is materially more work for this phase (distroless deferred either way).

### 5.3 Documentation

- Update **`docker/README.md`** for Phase B completion and cross-link **profile** layout.
- Expand root **`README.md`** with a **short** “run devnet” section pointing to profiles and Compose.

---

## 6. CI

- **`ci.yml`:** use **1.88** (or **`dtolnay/rust-toolchain` from `rust-toolchain.toml`**) for **fmt**, **clippy**, **build**, **test** so local and CI match.
- **`docker-smoke.yml`:** after **`node`** is default, build image with **`node`**, run container(s), **wait for readiness HTTP**, assert with **curl** or **`docker healthcheck`** logs; keep a **fast** path (single container) if multi-node smoke is flaky — **multi-node smoke** is **desired** but may be phased if the plan identifies stability risk.

---

## 7. Testing strategy

- **Unit:** bridge encode/decode, config merge order, profile selection.
- **Integration:** at least one test or harness that spins **multiple local tasks** or uses **test libp2p** memory transport **if** it reduces flake vs full UDP/QUIC in CI (implementation decides; the plan must name the chosen approach).
- **Manual / Compose:** four-node happy path documented as the **acceptance** demo.

---

## 8. Acceptance criteria

- From a clean clone, a contributor can follow docs to run **4 nodes** (Compose or documented local equivalent) with **the same `node` binary and profile mechanism** intended to extend to testnet — **with no manual edit of the profile between first boot and a working network** (peer IDs are deterministic per §3.4).
- **`[net]`** is loaded from the layered config; bootstrap addresses work across containers.
- Broadcast **`Action`**s on the default devnet path result in **actual gossip publish**, and inbound messages **reach** the consensus driver (verified by test and/or observed metrics/logs per implementation plan).
- **Skeleton mode is opt-in:** the default **`devnet`** profile fails closed (startup error, not silent fallback) if the swarm task is not wired up or the network path is missing — verified by a negative test that disables the swarm and asserts the node refuses to start.
- **Docker image** builds **`node`**; Compose **healthcheck** passes when the admin server is ready.
- **CI** uses Rust **1.88** consistently; Docker workflow validates **`node`** image at least at **single-replica** readiness, with a documented path to **4-replica** smoke.

---

## 9. Phased rollout (implementation ordering)

1. **Config:** layered TOML + **`[net]`** in `NodeConfig`; profile CLI/env; node-identity scheme per §3.4; remove hidden `NetConfig::devnet_default()` as the sole runtime source.
2. **Network:** libp2p swarm task + wire inbound/outbound; transport per §4.1; make skeleton mode opt-in (with the §8 negative test).
3. **Docker:** Dockerfile **`node`**, Compose ports/bootstrap/health. **`lua_dag_smoke` disposition is a binding decision in the plan** — pick exactly one: **(a) delete it** entirely, or **(b) retain it as a Rocks-only CI check** under a clearly distinct workflow name. Recommend **(a)** unless the plan identifies a concrete coverage gap. "Defer to later" is not an acceptable answer.
4. **Docs and secrets layout:** **`.example`** identity/key files per §3.3 and §3.4; documented mount points; `docker/README.md` Phase B update; root `README.md` "run devnet" section; profile-layout cross-link.
5. **CI:** toolchain alignment to **1.88**; `docker-smoke.yml` updated for `node` image; single-replica readiness gate; documented path to 4-replica smoke.

Dependencies: **(1)** before **(2)** is strongly recommended so listen/bootstrap are not re-hard-coded during swarm work. **(4)** depends on **(3)** (mount paths and image layout must be settled first).

---

## 10. Self-review

| Check | Result |
|-------|--------|
| Placeholder scan | No `TBD`; optional implementation choices (e.g. exact env names) called out as “implementation defines spelling in plan” where needed |
| Internal consistency | Aligns with prior Docker spec; extends Phase B; `sim` scope preserved |
| Scope | Single coherent devnet deliverable; large items (K8s, ceremony) explicitly out of scope |
| Ambiguity | Bootstrap env **replace vs append** binding decision is now anchored in §3.2 (recommendation: replace); §3.1 fixes array-merge semantics; §3.4 fixes node-identity scheme; §4.1 fixes transport. Remaining open items (gossip topics, exact env var spelling) are explicitly scoped to the implementation plan. |

---

## 11. Next step

After review of this file, create **`docs/superpowers/plans/...`** via the **writing-plans** workflow (task-sized steps, file-level ownership, test/CI commands).
